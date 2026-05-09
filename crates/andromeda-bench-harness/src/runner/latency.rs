use andromeda_bench_workload::{BenchmarkError, compute_percentile};
use andromeda_scenario_evidence::{BenchmarkMeasurementMode, BenchmarkWorkloadCounter};

use super::counters::validate_workload_counters;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatencyEvidence {
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub measurement_mode: BenchmarkMeasurementMode,
    pub latency_source: &'static str,
    pub engine_harness: Option<&'static str>,
    pub synthetic_model_version: Option<&'static str>,
    pub workload_counters: Vec<BenchmarkWorkloadCounter>,
}

pub fn harness_latency_evidence(
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
