mod common;
#[cfg(test)]
mod tests;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use common::{
    datum_to_key, empty_histogram, estimate_ndv, infer_skew, keyable_keys, single_bucket_histogram,
    sort_values_by_key, stats_validation_error,
};

use super::{HistogramBucket, HistogramPlaceholder, SkewMarker};

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

pub struct EquiWidthHistogramBuilder {
    bucket_count: u32,
    values: Vec<Datum>,
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
        })
    }

    pub fn finalize(self) -> AndromedaResult<HistogramPlaceholder> {
        HistogramBuilderTrait::finalize(Box::new(self))
    }
}

impl HistogramBuilderTrait for EquiWidthHistogramBuilder {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()> {
        if matches!(value, Datum::Null) {
            return Ok(());
        }
        self.values.push(value.clone());
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> AndromedaResult<HistogramPlaceholder> {
        if self.values.is_empty() {
            return empty_histogram();
        }

        sort_values_by_key(&mut self.values);

        let ndv = estimate_ndv(self.values.iter());
        let mut keys = keyable_keys(&self.values);

        if keys.is_empty() {
            return single_bucket_histogram(
                0,
                0,
                self.values.len() as u64,
                ndv,
                SkewMarker::Unknown,
            );
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
                let bucket_ndv = estimate_ndv(bucket_values.iter().copied());

                buckets.push(HistogramBucket {
                    lower_inclusive: lower_key,
                    upper_inclusive: upper_key,
                    row_estimate: bucket_values.len() as u64,
                    distinct_estimate: bucket_ndv,
                });
            }
        }

        if buckets.is_empty() {
            return single_bucket_histogram(
                min_key,
                max_key,
                self.values.len() as u64,
                ndv,
                SkewMarker::Unknown,
            );
        }

        HistogramPlaceholder::new(buckets, infer_skew(&self.values)).map_err(stats_validation_error)
    }

    fn estimated_memory_bytes(&self) -> u64 {
        (self.values.len() as u64) * 48
    }
}

pub struct EquiDepthHistogramBuilder {
    bucket_count: u32,
    values: Vec<Datum>,
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
        })
    }

    pub fn finalize(self) -> AndromedaResult<HistogramPlaceholder> {
        HistogramBuilderTrait::finalize(Box::new(self))
    }
}

impl HistogramBuilderTrait for EquiDepthHistogramBuilder {
    fn add_value(&mut self, value: &Datum) -> AndromedaResult<()> {
        if matches!(value, Datum::Null) {
            return Ok(());
        }
        self.values.push(value.clone());
        Ok(())
    }

    fn finalize(mut self: Box<Self>) -> AndromedaResult<HistogramPlaceholder> {
        if self.values.is_empty() {
            return empty_histogram();
        }

        sort_values_by_key(&mut self.values);

        let ndv = estimate_ndv(self.values.iter());
        let mut buckets = Vec::new();
        let bucket_count = (self.bucket_count as usize).min(self.values.len());

        if keyable_keys(&self.values).is_empty() {
            return single_bucket_histogram(
                0,
                0,
                self.values.len() as u64,
                ndv,
                SkewMarker::Unknown,
            );
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
            let bucket_ndv = estimate_ndv(bucket_values.iter());

            let Some(first_value) = bucket_values.first() else {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "EquiDepthHistogramBuilder: generated an empty bucket",
                ));
            };
            let Some(last_value) = bucket_values.last() else {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "EquiDepthHistogramBuilder: generated an empty bucket",
                ));
            };

            let lower_key = datum_to_key(first_value).unwrap_or(0);
            let upper_key = datum_to_key(last_value).unwrap_or(0);

            buckets.push(HistogramBucket {
                lower_inclusive: lower_key,
                upper_inclusive: upper_key,
                row_estimate: bucket_values.len() as u64,
                distinct_estimate: bucket_ndv,
            });

            start_idx = end_idx;
        }

        if buckets.is_empty() {
            return single_bucket_histogram(
                0,
                0,
                self.values.len() as u64,
                ndv,
                SkewMarker::Unknown,
            );
        }

        HistogramPlaceholder::new(buckets, infer_skew(&self.values)).map_err(stats_validation_error)
    }

    fn estimated_memory_bytes(&self) -> u64 {
        (self.values.len() as u64) * 48
    }
}
