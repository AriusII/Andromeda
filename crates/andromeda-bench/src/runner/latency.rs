use crate::{
    BenchmarkError, BenchmarkMeasurementMode, BenchmarkWorkloadCounter, compute_percentile,
};

use super::counters::validate_workload_counters;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LatencyEvidence {
    pub(super) p50_latency_us: u64,
    pub(super) p95_latency_us: u64,
    pub(super) measurement_mode: BenchmarkMeasurementMode,
    pub(super) latency_source: &'static str,
    pub(super) engine_harness: Option<&'static str>,
    pub(super) synthetic_model_version: Option<&'static str>,
    pub(super) workload_counters: Vec<BenchmarkWorkloadCounter>,
}

pub(super) fn harness_latency_evidence(
    mut latencies: Vec<u64>,
    latency_source: &'static str,
    engine_harness: &'static str,
    workload_counters: Vec<BenchmarkWorkloadCounter>,
) -> Result<LatencyEvidence, BenchmarkError> {
    if latencies.is_empty() {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }
    validate_workload_counters(&workload_counters)?;
    latencies.sort_unstable();
    Ok(LatencyEvidence {
        p50_latency_us: compute_percentile(&latencies, 50.0),
        p95_latency_us: compute_percentile(&latencies, 95.0),
        measurement_mode: BenchmarkMeasurementMode::HarnessDiagnostic,
        latency_source,
        engine_harness: Some(engine_harness),
        synthetic_model_version: None,
        workload_counters,
    })
}
