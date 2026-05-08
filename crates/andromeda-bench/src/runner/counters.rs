use crate::{
    BenchmarkError, BenchmarkRunRequest, BenchmarkWorkloadCounter, MAX_BENCHMARK_WORKLOAD_COUNTERS,
};

pub(super) fn validate_workload_counters(
    workload_counters: &[BenchmarkWorkloadCounter],
) -> Result<(), BenchmarkError> {
    if workload_counters.len() > MAX_BENCHMARK_WORKLOAD_COUNTERS
        || !workload_counters.iter().all(|counter| counter.is_valid())
    {
        return Err(BenchmarkError::HarnessFailed);
    }
    Ok(())
}

pub(super) fn requested_sample_counters(
    request: &BenchmarkRunRequest,
) -> Vec<BenchmarkWorkloadCounter> {
    vec![
        BenchmarkWorkloadCounter::new("requested_samples", u64::from(request.samples), "samples"),
        BenchmarkWorkloadCounter::new("requested_warmups", u64::from(request.warmups), "warmups"),
    ]
}

pub(super) fn counters_from_usize(
    counters: &[(&'static str, usize, &'static str)],
) -> Result<Vec<BenchmarkWorkloadCounter>, BenchmarkError> {
    counters
        .iter()
        .map(|(name, value, unit)| counter_from_usize(*name, *value, *unit))
        .collect()
}

pub(super) fn counter_from_usize(
    name: &'static str,
    value: usize,
    unit: &'static str,
) -> Result<BenchmarkWorkloadCounter, BenchmarkError> {
    let value = u64::try_from(value).map_err(|_| BenchmarkError::HarnessFailed)?;
    Ok(BenchmarkWorkloadCounter::new(name, value, unit))
}
