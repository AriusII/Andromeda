use std::collections::HashSet;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::super::{HistogramBucket, HistogramPlaceholder, SkewMarker, StatsValidationError};
use super::Datum;

pub(super) fn datum_to_key(value: &Datum) -> AndromedaResult<u64> {
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
        Datum::Bool(v) => Ok(u64::from(*v)),
        Datum::Bytes(_) | Datum::Text(_) => Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "datum_to_key: unsupported type for histogram bucketing",
        )),
    }
}

pub(super) fn sort_values_by_key(values: &mut [Datum]) {
    values.sort_by(|a, b| {
        let a_key = datum_to_key(a);
        let b_key = datum_to_key(b);
        match (a_key, b_key) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => format!("{a:?}").cmp(&format!("{b:?}")),
        }
    });
}

pub(super) fn keyable_keys(values: &[Datum]) -> Vec<u64> {
    values
        .iter()
        .filter_map(|datum| datum_to_key(datum).ok())
        .collect()
}

pub(super) fn estimate_ndv<'a>(values: impl IntoIterator<Item = &'a Datum>) -> u64 {
    let mut seen = HashSet::new();
    for value in values {
        seen.insert(format!("{value:?}"));
    }
    seen.len() as u64
}

pub(super) fn infer_skew(values: &[Datum]) -> SkewMarker {
    if values.is_empty() {
        return SkewMarker::Unknown;
    }

    let ndv_ratio = estimate_ndv(values.iter()) as f64 / values.len() as f64;

    match ndv_ratio {
        r if r > 0.9 => SkewMarker::Uniform,
        r if r > 0.7 => SkewMarker::LowSkew,
        r if r > 0.3 => SkewMarker::ModerateSkew,
        r if r > 0.05 => SkewMarker::HighSkew,
        _ => SkewMarker::HeavyHitter,
    }
}

pub(super) fn empty_histogram() -> AndromedaResult<HistogramPlaceholder> {
    single_bucket_histogram(0, 0, 0, 0, SkewMarker::Unknown)
}

pub(super) fn single_bucket_histogram(
    lower_inclusive: u64,
    upper_inclusive: u64,
    row_estimate: u64,
    distinct_estimate: u64,
    skew: SkewMarker,
) -> AndromedaResult<HistogramPlaceholder> {
    HistogramPlaceholder::new(
        vec![HistogramBucket {
            lower_inclusive,
            upper_inclusive,
            row_estimate,
            distinct_estimate,
        }],
        skew,
    )
    .map_err(stats_validation_error)
}

pub(super) fn stats_validation_error(err: StatsValidationError) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Catalog, err.to_string())
}
