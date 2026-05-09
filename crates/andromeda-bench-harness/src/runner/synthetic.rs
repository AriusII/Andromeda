use andromeda_bench_workload::{
    BenchmarkError, BenchmarkHardwareProfile, BenchmarkRunRequest,
    INVENTORY_RECOVERABLE_SMOKE_WORKLOAD_ID, PROTOCOL_SMOKE_CONTRACT_WORKLOAD_ID,
    WAL_APPEND_SMOKE_WORKLOAD_ID,
};
use andromeda_scenario_evidence::BenchmarkMeasurementMode;

use super::counters::requested_sample_counters;
use super::latency::LatencyEvidence;

pub const SYNTHETIC_LATENCY_SOURCE: &str = "deterministic-latency-model";
pub const SYNTHETIC_MODEL_VERSION: &str = "bounded-diagnostic-v1";

pub fn synthetic_latency_evidence(
    workload_id: &str,
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let profile_adjustment_us = match request.hardware_profile {
        BenchmarkHardwareProfile::Conservative => 0,
        BenchmarkHardwareProfile::DeclaredLocal => 1,
    };
    let workload_base_latency_us = match workload_id {
        INVENTORY_RECOVERABLE_SMOKE_WORKLOAD_ID => 2_500,
        PROTOCOL_SMOKE_CONTRACT_WORKLOAD_ID => 1_000,
        WAL_APPEND_SMOKE_WORKLOAD_ID => 1_500,
        _ => return Err(BenchmarkError::UnknownWorkload),
    };

    let p50_latency_us = workload_base_latency_us
        + u64::from(request.samples)
        + u64::from(request.warmups)
        + profile_adjustment_us;
    let p95_latency_us = p50_latency_us * 2 + request.duration_ms / 1_000;

    Ok(LatencyEvidence {
        p50_latency_us,
        p95_latency_us,
        measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
        latency_source: SYNTHETIC_LATENCY_SOURCE,
        engine_harness: None,
        synthetic_model_version: Some(SYNTHETIC_MODEL_VERSION),
        workload_counters: requested_sample_counters(request),
    })
}
