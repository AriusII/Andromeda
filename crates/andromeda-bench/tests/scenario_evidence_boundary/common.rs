use andromeda_bench_workload::DEFAULT_TEMP_BYTES;
use andromeda_scenario_evidence::{
    BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence, BenchmarkEvidenceValidity,
    BenchmarkHistoryRecord, BenchmarkPlanClass, BenchmarkScenarioEvidence,
    BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget, BenchmarkStatsVersion,
};
use andromeda_time::EngineTimestamp;
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

pub(crate) fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

pub(crate) fn target() -> BenchmarkScenarioTarget {
    target_with_stats(1)
}

pub(crate) fn target_with_stats(stats_version: u64) -> BenchmarkScenarioTarget {
    BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(9),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(stats_version),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap()
}

pub(crate) fn validity() -> BenchmarkEvidenceValidity {
    BenchmarkEvidenceValidity::new(ts(1_000), ts(86_401_000)).unwrap()
}

pub(crate) fn history_record_with_metrics(
    workload_id: &str,
    p50_latency_us: u64,
    p95_latency_us: u64,
    error_count: u32,
    sample_count: u32,
) -> BenchmarkHistoryRecord {
    BenchmarkHistoryRecord::new(
        workload_id.to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        p50_latency_us,
        p95_latency_us,
        error_count,
        sample_count,
    )
}

pub(crate) fn vertical_history_record(sample_count: u32) -> BenchmarkHistoryRecord {
    history_record_with_metrics(
        "inventory-recoverable-smoke",
        15_000,
        50_000,
        0,
        sample_count,
    )
}

pub(crate) fn default_budgets() -> BenchmarkEvidenceBudgets {
    BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap()
}

pub(crate) fn default_confidence() -> BenchmarkEvidenceConfidence {
    BenchmarkEvidenceConfidence::from_permille(850).unwrap()
}

pub(crate) fn boundary_from_history(
    record: &BenchmarkHistoryRecord,
    budgets: BenchmarkEvidenceBudgets,
) -> Result<BenchmarkScenarioEvidence, BenchmarkScenarioEvidenceError> {
    record.to_scenario_evidence_boundary(target(), budgets, default_confidence(), validity())
}
