#![forbid(unsafe_code)]

//! Advisory Andromeda benchmark regression analysis.
//!
//! This crate owns baseline comparison and regression reporting. Regression
//! reports are diagnostic evidence only and cannot drive optimizer, storage,
//! WAL, recovery, catalog, or security decisions by themselves.

mod regression_detection;

mod metric_math {
    pub(crate) use andromeda_bench_workload::{error_rate_ppm, percent_change};
}

pub use andromeda_bench_workload::{
    AUDIT_APPEND_FILE_SINK_SMOKE_WORKLOAD_ID, BENCHMARK_BUDGET_ORIGIN, BENCHMARK_DECISION_LINKAGE,
    BENCHMARK_PRIMARY_METRIC, BTREE_LOOKUP_SMOKE_WORKLOAD_ID, BTREE_NODE_CODEC_SMOKE_WORKLOAD_ID,
    BTREE_RANGE_SCAN_SMOKE_WORKLOAD_ID, BenchmarkError, BenchmarkHardwareProfile,
    BenchmarkRunRequest, BenchmarkWorkload, BenchmarkWorkloadClass, BudgetStatus,
    DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_TEMP_BYTES, DEFAULT_WARMUPS, MAX_DURATION_MS,
    MAX_EVIDENCE_TTL_MS, MAX_SAMPLES, MAX_TEMP_BYTES, MAX_WARMUPS,
    PROTOCOL_SMOKE_CONTRACT_WORKLOAD_ID, PerformanceBudget, RECOVERY_REPLAY_WAL_SMOKE_WORKLOAD_ID,
    SRPL_COMPILE_OPTIMIZE_SMOKE_WORKLOAD_ID, STORAGE_PAGE_STORE_SMOKE_WORKLOAD_ID,
    VERTICAL_V0_SMOKE_WORKLOAD_ID, WAL_APPEND_FILE_SMOKE_WORKLOAD_ID, WAL_APPEND_SMOKE_WORKLOAD_ID,
    WORKLOADS, evaluate_budget, find_workload, validate_run_request,
};
pub use andromeda_scenario_evidence::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
    BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER, BenchmarkEvidence,
    BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence, BenchmarkEvidenceContext,
    BenchmarkEvidenceValidity, BenchmarkHistoryAdvisoryMetadata, BenchmarkHistoryRecord,
    BenchmarkHistoryStore, BenchmarkMeasurementMode, BenchmarkPlanClass, BenchmarkScenarioEvidence,
    BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget, BenchmarkStatsVersion,
    BenchmarkWorkloadCounter, HistoryQuery, HistoryQueryResult, MAX_BENCHMARK_WORKLOAD_COUNTERS,
    TimeRange,
};
pub use regression_detection::{
    BenchmarkBaseline, BenchmarkBaselineComparisonError, BenchmarkBaselineContext,
    RegressionAnalysis, RegressionReason,
};
