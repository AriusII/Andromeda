use andromeda_bench::{
    BenchmarkEvidenceConfidence, BenchmarkMeasurementMode, BenchmarkRunRequest,
    BenchmarkScenarioEvidence, DEFAULT_TEMP_BYTES, run_bounded_benchmark,
};

use crate::common::{target, validity};

#[test]
fn benchmark_run_evidence_preserves_explicit_generation_budgets() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.duration_ms = 1_000;
    request.samples = 5;
    request.warmups = 1;
    request.temp_budget_bytes = DEFAULT_TEMP_BYTES / 2;

    let evidence = run_bounded_benchmark(&request).unwrap();
    let boundary = BenchmarkScenarioEvidence::from_benchmark_evidence(
        &evidence,
        "commit-20260506",
        "2026-05-06T10:00:00Z",
        target(),
        BenchmarkEvidenceConfidence::from_permille(700).unwrap(),
        validity(),
    )
    .unwrap();

    assert_eq!(boundary.sample_count(), 5);
    assert_eq!(boundary.budgets().duration_ms, 1_000);
    assert_eq!(boundary.budgets().samples, 5);
    assert_eq!(boundary.budgets().temp_bytes, DEFAULT_TEMP_BYTES / 2);
    assert_eq!(
        boundary.context().hardware_profile(),
        Some(request.hardware_profile)
    );
    assert_eq!(
        boundary.context().measurement_mode(),
        Some(BenchmarkMeasurementMode::SyntheticDiagnostic)
    );
    assert_eq!(
        boundary.context().latency_source(),
        Some("deterministic-latency-model")
    );
    assert_eq!(
        boundary.context().timing_source(),
        Some(andromeda_bench::BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER)
    );
    assert!(
        boundary
            .to_json()
            .contains(r#""timing_source":"deterministic-run-clock-placeholder""#)
    );
    assert!(!boundary.is_authoritative());
}
