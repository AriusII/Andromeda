use crate::{BenchmarkHardwareProfile, BudgetStatus};

use super::*;

#[test]
fn diagnostic_evidence_is_explicit() {
    let evidence = BenchmarkEvidence {
        workload_id: "protocol-smoke-contract".to_string(),
        workload_hypothesis: "typed protocol contract inspection should stay bounded without opening a network surface",
        workload_shape_version: "protocol-smoke-contract.synthetic.v1",
        workload_size: "contract-only synthetic inspection, samples<=20, duration_ms<=5000",
        primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
        baseline_ref: "history.protocol-smoke-contract.synthetic.v1",
        budget_origin: "static-workload-registry-v1",
        decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
        hardware_profile: BenchmarkHardwareProfile::Conservative,
        duration_ms: 1_000,
        samples: 5,
        warmups: 1,
        temp_budget_bytes: 8 * 1024 * 1024,
        started_at_unix_ms: 1,
        elapsed_ms: 950,
        sample_count: 5,
        p50_latency_us: 10,
        p95_latency_us: 20,
        error_count: 0,
        budget_status: BudgetStatus::Passed,
        diagnostic_only: true,
        measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
        latency_source: "deterministic-latency-model",
        timing_source: BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER,
        engine_harness: None,
        synthetic_model_version: Some("bounded-diagnostic-v1"),
        workload_counters: vec![
            BenchmarkWorkloadCounter::new("requested_samples", 5, "samples"),
            BenchmarkWorkloadCounter::new("requested_warmups", 1, "warmups"),
        ],
        commit_sha: None,
        rustc_version: "unknown",
        process_pid: 0,
    };

    assert!(evidence.diagnostic_only);
    assert!(evidence.workload_hypothesis.contains("protocol contract"));
    assert_eq!(
        evidence.workload_shape_version,
        "protocol-smoke-contract.synthetic.v1"
    );
    assert!(evidence.workload_size.contains("samples<=20"));
    assert_eq!(
        evidence.primary_metric,
        "p50_latency_us,p95_latency_us,error_rate_ppm"
    );
    assert_eq!(
        evidence.baseline_ref,
        "history.protocol-smoke-contract.synthetic.v1"
    );
    assert_eq!(evidence.budget_origin, "static-workload-registry-v1");
    assert!(evidence.decision_linkage.contains("ContractHash"));
    assert_eq!(evidence.hardware_profile.as_str(), "conservative");
    assert_eq!(evidence.measurement_mode.as_str(), "synthetic-diagnostic");
    assert_eq!(
        evidence.timing_source,
        BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER
    );
    assert_eq!(evidence.engine_harness, None);
    assert_eq!(
        evidence.synthetic_model_version,
        Some("bounded-diagnostic-v1")
    );
    assert_eq!(evidence.workload_counters.len(), 2);
    assert!(
        evidence
            .workload_counters
            .iter()
            .all(|counter| counter.is_valid())
    );
    assert!(!evidence.is_authoritative());
    assert!(!evidence.can_select_plan_alone());
    assert_eq!(evidence.optimizer_consumption_role(), "advisory-only");
}
