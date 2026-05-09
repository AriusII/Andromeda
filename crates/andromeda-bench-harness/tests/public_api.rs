use std::time::Instant;

use andromeda_bench_harness::{
    BenchmarkTempDir, BenchmarkTempFile, LatencyEvidence, SYNTHETIC_LATENCY_SOURCE,
    SYNTHETIC_MODEL_VERSION, elapsed_micros, requested_sample_counters, synthetic_latency_evidence,
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
