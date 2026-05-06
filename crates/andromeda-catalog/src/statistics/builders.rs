use std::collections::HashSet;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::{HistogramBucket, HistogramPlaceholder, SkewMarker, StatsValidationError};

#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Null,
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(f32),
    Float64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Text(String),
}

pub trait HistogramBuilderTrait: Send + Sync {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()>;
    fn finalize(self: Box<Self>) -> AndromedaResult<HistogramPlaceholder>;
    fn estimated_memory_bytes(&self) -> u64;
}

fn datum_to_key(value: &Datum) -> AndromedaResult<u64> {
    match value {
        Datum::Null => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "datum_to_key: cannot key null value",
        )),
        Datum::Int8(v) => Ok((*v as i64) as u64),
        Datum::Int16(v) => Ok((*v as i64) as u64),
        Datum::Int32(v) => Ok((*v as i64) as u64),
        Datum::Int64(v) => Ok(*v as u64),
        Datum::UInt8(v) => Ok(*v as u64),
        Datum::UInt16(v) => Ok(*v as u64),
        Datum::UInt32(v) => Ok(*v as u64),
        Datum::UInt64(v) => Ok(*v),
        Datum::Float32(v) => Ok(v.to_bits() as u64),
        Datum::Float64(v) => Ok(v.to_bits()),
        Datum::Bool(v) => Ok(if *v { 1 } else { 0 }),
        Datum::Bytes(_) | Datum::Text(_) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "datum_to_key: unsupported type for histogram bucketing",
        )),
    }
}

fn stats_validation_error(err: StatsValidationError) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Catalog, err.to_string())
}

pub struct EquiWidthHistogramBuilder {
    bucket_count: u32,
    values: Vec<Datum>,
    null_count: u64,
}

impl EquiWidthHistogramBuilder {
    pub fn new(bucket_count: u32) -> AndromedaResult<Self> {
        if bucket_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "EquiWidthHistogramBuilder: bucket_count must be > 0",
            ));
        }
        Ok(Self {
            bucket_count,
            values: Vec::new(),
            null_count: 0,
        })
    }

    pub fn finalize(self) -> AndromedaResult<HistogramPlaceholder> {
        HistogramBuilderTrait::finalize(Box::new(self))
    }

    fn estimate_ndv(values: &[Datum]) -> AndromedaResult<u64> {
        let mut seen = HashSet::new();
        for v in values {
            let key = format!("{:?}", v);
            seen.insert(key);
        }
        Ok(seen.len() as u64)
    }

    fn infer_skew(values: &[Datum]) -> SkewMarker {
        if values.is_empty() {
            return SkewMarker::Unknown;
        }

        let ndv_exact = {
            let mut s = HashSet::new();
            for v in values {
                s.insert(format!("{:?}", v));
            }
            s.len() as f64
        };

        let total = values.len() as f64;
        let ndv_ratio = ndv_exact / total;

        match ndv_ratio {
            r if r > 0.9 => SkewMarker::Uniform,
            r if r > 0.7 => SkewMarker::LowSkew,
            r if r > 0.3 => SkewMarker::ModerateSkew,
            r if r > 0.05 => SkewMarker::HighSkew,
            _ => SkewMarker::HeavyHitter,
        }
    }
}

impl HistogramBuilderTrait for EquiWidthHistogramBuilder {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()> {
        if matches!(value, Datum::Null) {
            self.null_count += 1;
        } else {
            self.values.push(value.clone());
        }
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> AndromedaResult<HistogramPlaceholder> {
        if self.values.is_empty() {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: 0,
                    distinct_estimate: 0,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        self.values.sort_by(|a, b| {
            let a_key = datum_to_key(a);
            let b_key = datum_to_key(b);
            match (a_key, b_key) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                _ => format!("{:?}", a).cmp(&format!("{:?}", b)),
            }
        });

        let ndv = Self::estimate_ndv(&self.values)?;

        let mut keys: Vec<u64> = Vec::new();
        for datum in &self.values {
            if let Ok(k) = datum_to_key(datum) {
                keys.push(k);
            }
        }

        if keys.is_empty() {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: self.values.len() as u64,
                    distinct_estimate: ndv,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        keys.sort_unstable();

        let min_key = keys.first().copied().unwrap_or(0);
        let max_key = keys.last().copied().unwrap_or(0);

        let bucket_count = (self.bucket_count as usize).min(self.values.len());
        let key_range = max_key.saturating_sub(min_key).saturating_add(1);
        let bucket_width = key_range.div_ceil(bucket_count as u64).max(1);

        let mut buckets = Vec::new();

        for b in 0..bucket_count {
            let lower_key = min_key.saturating_add((b as u64).saturating_mul(bucket_width));
            let upper_key = if b == bucket_count - 1 {
                max_key
            } else {
                min_key
                    .saturating_add(((b + 1) as u64).saturating_mul(bucket_width))
                    .saturating_sub(1)
            };

            let bucket_values: Vec<&Datum> = self
                .values
                .iter()
                .filter(|v| {
                    if let Ok(k) = datum_to_key(v) {
                        k >= lower_key && k <= upper_key
                    } else {
                        false
                    }
                })
                .collect();

            if !bucket_values.is_empty() {
                let bucket_ndv = Self::estimate_ndv(
                    &bucket_values
                        .iter()
                        .map(|v| (*v).clone())
                        .collect::<Vec<_>>(),
                )?;

                buckets.push(HistogramBucket {
                    lower_inclusive: lower_key,
                    upper_inclusive: upper_key,
                    row_estimate: bucket_values.len() as u64,
                    distinct_estimate: bucket_ndv,
                });
            }
        }

        if buckets.is_empty() {
            buckets.push(HistogramBucket {
                lower_inclusive: min_key,
                upper_inclusive: max_key,
                row_estimate: self.values.len() as u64,
                distinct_estimate: ndv,
            });
        }

        HistogramPlaceholder::new(buckets, Self::infer_skew(&self.values))
            .map_err(stats_validation_error)
    }

    fn estimated_memory_bytes(&self) -> u64 {
        (self.values.len() as u64) * 48
    }
}

pub struct EquiDepthHistogramBuilder {
    bucket_count: u32,
    values: Vec<Datum>,
    null_count: u64,
}

impl EquiDepthHistogramBuilder {
    pub fn new(bucket_count: u32) -> AndromedaResult<Self> {
        if bucket_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "EquiDepthHistogramBuilder: bucket_count must be > 0",
            ));
        }
        Ok(Self {
            bucket_count,
            values: Vec::new(),
            null_count: 0,
        })
    }

    pub fn finalize(self) -> AndromedaResult<HistogramPlaceholder> {
        HistogramBuilderTrait::finalize(Box::new(self))
    }

    fn estimate_ndv(values: &[Datum]) -> AndromedaResult<u64> {
        let mut seen = HashSet::new();
        for v in values {
            let key = format!("{:?}", v);
            seen.insert(key);
        }
        Ok(seen.len() as u64)
    }

    fn infer_skew(values: &[Datum]) -> SkewMarker {
        if values.is_empty() {
            return SkewMarker::Unknown;
        }

        let ndv_exact = {
            let mut s = HashSet::new();
            for v in values {
                s.insert(format!("{:?}", v));
            }
            s.len() as f64
        };

        let total = values.len() as f64;
        let ndv_ratio = ndv_exact / total;

        match ndv_ratio {
            r if r > 0.9 => SkewMarker::Uniform,
            r if r > 0.7 => SkewMarker::LowSkew,
            r if r > 0.3 => SkewMarker::ModerateSkew,
            r if r > 0.05 => SkewMarker::HighSkew,
            _ => SkewMarker::HeavyHitter,
        }
    }
}

impl HistogramBuilderTrait for EquiDepthHistogramBuilder {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()> {
        if matches!(value, Datum::Null) {
            self.null_count += 1;
        } else {
            self.values.push(value.clone());
        }
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> AndromedaResult<HistogramPlaceholder> {
        if self.values.is_empty() {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: 0,
                    distinct_estimate: 0,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        self.values.sort_by(|a, b| {
            let a_key = datum_to_key(a);
            let b_key = datum_to_key(b);
            match (a_key, b_key) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                _ => format!("{:?}", a).cmp(&format!("{:?}", b)),
            }
        });

        let ndv = Self::estimate_ndv(&self.values)?;
        let mut buckets = Vec::new();
        let bucket_count = (self.bucket_count as usize).min(self.values.len());

        if bucket_count == 0 {
            return HistogramPlaceholder::new(
                vec![HistogramBucket {
                    lower_inclusive: 0,
                    upper_inclusive: 0,
                    row_estimate: 0,
                    distinct_estimate: 0,
                }],
                SkewMarker::Unknown,
            )
            .map_err(stats_validation_error);
        }

        let mut start_idx = 0usize;

        while start_idx < self.values.len() && buckets.len() < bucket_count {
            let remaining_rows = self.values.len() - start_idx;
            let remaining_buckets = bucket_count - buckets.len();
            let target_size = remaining_rows.div_ceil(remaining_buckets);
            let mut end_idx = (start_idx + target_size).min(self.values.len());

            if end_idx < self.values.len() {
                let boundary_key = datum_to_key(&self.values[end_idx - 1]).ok();
                while end_idx < self.values.len()
                    && datum_to_key(&self.values[end_idx]).ok() == boundary_key
                {
                    end_idx += 1;
                }
            }

            let bucket_values = &self.values[start_idx..end_idx];
            let bucket_ndv = Self::estimate_ndv(bucket_values)?;

            let lower_key = datum_to_key(bucket_values.first().unwrap()).unwrap_or(0);
            let upper_key = datum_to_key(bucket_values.last().unwrap()).unwrap_or(0);

            buckets.push(HistogramBucket {
                lower_inclusive: lower_key,
                upper_inclusive: upper_key,
                row_estimate: bucket_values.len() as u64,
                distinct_estimate: bucket_ndv,
            });

            start_idx = end_idx;
        }

        if buckets.is_empty() {
            buckets.push(HistogramBucket {
                lower_inclusive: 0,
                upper_inclusive: 0,
                row_estimate: self.values.len() as u64,
                distinct_estimate: ndv,
            });
        }

        HistogramPlaceholder::new(buckets, Self::infer_skew(&self.values))
            .map_err(stats_validation_error)
    }

    fn estimated_memory_bytes(&self) -> u64 {
        (self.values.len() as u64) * 48
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_equiwidth_histogram_empty() {
        let builder = EquiWidthHistogramBuilder::new(4).unwrap();
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(!histo.buckets().is_empty());
    }

    #[test]
    fn test_equiwidth_histogram_integers() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(!histo.buckets().is_empty());
        assert!(histo.buckets().iter().any(|b| b.row_estimate > 0));
    }

    #[test]
    fn test_equiwidth_with_nulls() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=50 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        for _ in 0..50 {
            builder.add_value(&Datum::Null).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
    }

    #[test]
    fn test_equidepth_histogram_integers() {
        let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(!histo.buckets().is_empty());
    }

    #[test]
    fn test_equidepth_balanced_buckets() {
        let mut builder = EquiDepthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        let total_rows: u64 = histo.buckets().iter().map(|b| b.row_estimate).sum();
        assert_eq!(total_rows, 100);
    }

    #[test]
    fn test_builder_zero_buckets_fails() {
        let result = EquiWidthHistogramBuilder::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_equidepth_builder_zero_buckets_fails() {
        let result = EquiDepthHistogramBuilder::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_mixed_numeric_types() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        builder.add_value(&Datum::Int32(10)).unwrap();
        builder.add_value(&Datum::Int64(20)).unwrap();
        builder.add_value(&Datum::UInt64(30)).unwrap();
        let result = builder.finalize();
        assert!(result.is_ok());
    }

    #[test]
    fn test_skew_inference_uniform() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=100 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();
        assert!(matches!(
            histo.skew(),
            SkewMarker::Uniform | SkewMarker::LowSkew
        ));
    }

    #[test]
    fn test_memory_estimation() {
        let mut builder = EquiWidthHistogramBuilder::new(4).unwrap();
        for i in 1..=1000 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let estimated = builder.estimated_memory_bytes();
        assert!(estimated > 0);
        assert!(estimated >= 1000 * 40);
    }

    #[test]
    fn test_histogram_validation_invariants() {
        let mut builder = EquiWidthHistogramBuilder::new(8).unwrap();
        for i in 1..=256 {
            builder.add_value(&Datum::Int64(i)).unwrap();
        }
        let result = builder.finalize();
        assert!(result.is_ok());
        let histo = result.unwrap();

        for bucket in histo.buckets() {
            assert!(bucket.lower_inclusive <= bucket.upper_inclusive);
            assert!(bucket.distinct_estimate <= bucket.row_estimate);
        }
    }
}
