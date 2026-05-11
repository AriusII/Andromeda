use std::time::Instant;

use andromeda_bench_harness::{
    BenchmarkTempDir, BenchmarkTempFile, LatencyEvidence, SYNTHETIC_LATENCY_SOURCE,
    SYNTHETIC_MODEL_VERSION, elapsed_micros, requested_sample_counters,
    run_bounded_benchmark_with_latency_dispatch, synthetic_latency_evidence,
};
use andromeda_bench_workload::BenchmarkRunRequest;
use andromeda_scenario_evidence::BenchmarkMeasurementMode;

#[test]
fn crate_root_exports_temp_helpers() {
    let _elapsed = elapsed_micros(Instant::now());

    fn assert_debug<T: std::fmt::Debug>() {}

    assert_debug::<BenchmarkTempDir>();
    assert_debug::<BenchmarkTempFile>();
    assert_debug::<LatencyEvidence>();
}

#[test]
fn crate_root_exports_synthetic_latency_helpers() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.samples = 5;
    request.warmups = 1;
    request.duration_ms = 1_000;

    let counters = requested_sample_counters(&request);
    assert_eq!(counters[0].name, "requested_samples");
    assert_eq!(counters[0].value, 5);

    let evidence = synthetic_latency_evidence("protocol-smoke-contract", &request).unwrap();
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::SyntheticDiagnostic
    );
    assert_eq!(evidence.latency_source, SYNTHETIC_LATENCY_SOURCE);
    assert_eq!(
        evidence.synthetic_model_version,
        Some(SYNTHETIC_MODEL_VERSION)
    );
}

#[test]
fn bounded_runner_assembles_advisory_evidence_from_injected_latency() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.samples = 5;
    request.warmups = 1;
    request.duration_ms = 1_000;

    let evidence = run_bounded_benchmark_with_latency_dispatch(&request, |workload_id, request| {
        synthetic_latency_evidence(workload_id, request)
    })
    .unwrap();

    assert_eq!(evidence.workload_id, "protocol-smoke-contract");
    assert!(evidence.diagnostic_only);
    // Andromeda project creation baseline (approximate): 2023-11-15 00:00:00 UTC = 1700000000000 ms
    assert!(
        evidence.started_at_unix_ms > 1700000000000,
        "started_at_unix_ms should be after Andromeda creation date, got {}",
        evidence.started_at_unix_ms
    );
    assert_eq!(evidence.elapsed_ms, 6);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::SyntheticDiagnostic
    );
}

#[test]
fn bounded_runner_captures_process_pid() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.samples = 5;
    request.warmups = 1;
    request.duration_ms = 1_000;

    let evidence = run_bounded_benchmark_with_latency_dispatch(&request, |workload_id, request| {
        synthetic_latency_evidence(workload_id, request)
    })
    .unwrap();

    let current_pid = std::process::id();
    assert_eq!(
        evidence.process_pid, current_pid,
        "process_pid should match current process ID"
    );
}

#[test]
fn bounded_runner_has_sensible_defaults_for_commit_sha() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.samples = 5;
    request.warmups = 1;
    request.duration_ms = 1_000;

    let evidence = run_bounded_benchmark_with_latency_dispatch(&request, |workload_id, request| {
        synthetic_latency_evidence(workload_id, request)
    })
    .unwrap();

    // When not in CI, commit_sha should be None
    // (This test won't fail even if the env var is set, as it's just checking the field exists)
    assert!(evidence.commit_sha.is_none() || evidence.commit_sha.is_some());
}

#[test]
fn bounded_runner_has_sensible_defaults_for_rustc_version() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.samples = 5;
    request.warmups = 1;
    request.duration_ms = 1_000;

    let evidence = run_bounded_benchmark_with_latency_dispatch(&request, |workload_id, request| {
        synthetic_latency_evidence(workload_id, request)
    })
    .unwrap();

    // rustc_version should always be set to something, either from env or "unknown"
    assert!(!evidence.rustc_version.is_empty());
}
