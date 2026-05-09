use andromeda_bench_workload::{BenchmarkError, BenchmarkRunRequest, evaluate_budget};
use andromeda_scenario_evidence::{
    BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER, BenchmarkEvidence,
};

use super::latency::LatencyEvidence;

pub fn run_bounded_benchmark_with_latency_dispatch(
    request: &BenchmarkRunRequest,
    dispatch_latency_evidence: impl FnOnce(
        &str,
        &BenchmarkRunRequest,
    ) -> Result<LatencyEvidence, BenchmarkError>,
) -> Result<BenchmarkEvidence, BenchmarkError> {
    let workload = request.validate()?;
    let _profile = request.hardware_profile.materialize();

    let sample_count = request.samples;
    let latency_evidence = dispatch_latency_evidence(workload.id, request)?;
    let error_count = 0;
    let budget_status = evaluate_budget(
        workload,
        latency_evidence.p50_latency_us,
        latency_evidence.p95_latency_us,
        error_count,
        sample_count,
    )?;
    let requested_iterations = u64::from(request.samples) + u64::from(request.warmups);
    let elapsed_ms = request.duration_ms.min(requested_iterations.max(1));

    Ok(BenchmarkEvidence {
        workload_id: workload.id.to_string(),
        workload_hypothesis: workload.hypothesis,
        workload_shape_version: workload.workload_shape_version,
        workload_size: workload.workload_size,
        primary_metric: workload.primary_metric,
        baseline_ref: workload.baseline_ref,
        budget_origin: workload.budget_origin,
        decision_linkage: workload.decision_linkage,
        hardware_profile: request.hardware_profile,
        duration_ms: request.duration_ms,
        samples: request.samples,
        warmups: request.warmups,
        temp_budget_bytes: request.temp_budget_bytes,
        started_at_unix_ms: 0,
        elapsed_ms,
        sample_count,
        p50_latency_us: latency_evidence.p50_latency_us,
        p95_latency_us: latency_evidence.p95_latency_us,
        error_count,
        budget_status,
        diagnostic_only: true,
        measurement_mode: latency_evidence.measurement_mode,
        latency_source: latency_evidence.latency_source,
        timing_source: BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER,
        engine_harness: latency_evidence.engine_harness,
        synthetic_model_version: latency_evidence.synthetic_model_version,
        workload_counters: latency_evidence.workload_counters,
    })
}
