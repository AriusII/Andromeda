/// Compute a finite percentage change from `baseline` to `current`.
///
/// A zero baseline has no mathematically meaningful percentage delta, but
/// benchmark evidence still needs deterministic regression and trend behavior.
/// Treat a move from zero to a positive value as a full finite regression and
/// zero to zero as unchanged.
pub(crate) fn percent_change(current: u64, baseline: u64) -> f64 {
    match (current, baseline) {
        (0, 0) => 0.0,
        (_, 0) => 100.0,
        _ => ((current as f64 - baseline as f64) / baseline as f64) * 100.0,
    }
}

pub(crate) fn error_rate_ppm(error_count: u32, sample_count: u32) -> u64 {
    if sample_count == 0 {
        return if error_count == 0 { 0 } else { 1_000_000 };
    }
    ((u64::from(error_count) * 1_000_000) / u64::from(sample_count)).min(1_000_000)
}
