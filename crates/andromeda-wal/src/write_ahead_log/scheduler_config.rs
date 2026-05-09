use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::time::Duration;

pub(crate) const UNLIMITED_SEGMENTS_PER_RUN: u64 = 0;

pub(crate) fn validate_nonzero_interval(
    interval: Duration,
    error_msg: &'static str,
) -> AndromedaResult<()> {
    if interval.is_zero() {
        return Err(storage_error(error_msg));
    }
    Ok(())
}

pub(crate) fn validate_ratio_inclusive(value: f64, error_msg: &'static str) -> AndromedaResult<()> {
    if !(0.0..=1.0).contains(&value) {
        return Err(storage_error(error_msg));
    }
    Ok(())
}

pub(crate) fn segments_per_run_limit(max_segments_per_run: u64, candidate_count: usize) -> usize {
    if max_segments_per_run == UNLIMITED_SEGMENTS_PER_RUN {
        candidate_count
    } else {
        (max_segments_per_run as usize).min(candidate_count)
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
