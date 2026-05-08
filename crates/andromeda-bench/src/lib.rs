#![forbid(unsafe_code)]

//! Bounded benchmark contracts for Andromeda operational diagnostics.
//!
//! This crate executes bounded benchmark smoke runs and reexports the
//! workload, evidence, and regression contracts owned by dedicated crates.

pub mod audit_file_benchmark;
pub mod btree_benchmark;
pub mod btree_node_codec_benchmark;
pub mod srpl_compiler_benchmark;
pub mod wal_file_benchmark;

mod crud;
mod flat_json;
mod runner;
mod storage_runtime_benchmark;

pub use andromeda_bench_workload::{
    BenchmarkError, BenchmarkHardwareProfile, BenchmarkRunRequest, BenchmarkWorkload,
    BenchmarkWorkloadClass, BudgetStatus, DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_TEMP_BYTES,
    DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_EVIDENCE_TTL_MS, MAX_SAMPLES, MAX_TEMP_BYTES,
    MAX_WARMUPS, PerformanceBudget, WORKLOADS, evaluate_budget, find_workload,
    validate_run_request,
};
pub use andromeda_regression::{
    BenchmarkBaseline, BenchmarkBaselineComparisonError, BenchmarkBaselineContext,
    RegressionAnalysis, RegressionReason,
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
pub use audit_file_benchmark::{
    AUDIT_APPEND_FILE_SINK_HARNESS_NAME, AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
    AUDIT_APPEND_FILE_SINK_WORKLOAD_ID, AuditAppendFileSinkSmokeBenchmark,
    run_audit_append_file_sink_smoke_benchmark,
};

pub use btree_benchmark::{
    BTreeBenchmarkConfig, BTreeBenchmarkContext, BTreeBenchmarkError, benchmark_btree_lookup,
    benchmark_btree_range_scan, setup_btree_lookup_harness, setup_btree_range_scan_harness,
};
pub use btree_node_codec_benchmark::{
    BTREE_NODE_CODEC_HARNESS_NAME, BTREE_NODE_CODEC_HARNESS_SOURCE, BTREE_NODE_CODEC_WORKLOAD_ID,
    BTreeNodeCodecSmokeBenchmark, run_btree_node_codec_smoke_benchmark,
};

pub use crud::{
    CRUD_SCENARIOS, CrudDataGenerator, CrudOperationMetrics, CrudRow, CrudScenarioDefinition,
    CrudWorkloadResult, MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS, MAX_CRUD_ROWS, MAX_CRUD_THREADS,
    compute_percentile, find_crud_scenario,
};
pub use runner::run_bounded_benchmark;
pub use srpl_compiler_benchmark::{
    SRPL_COMPILE_OPTIMIZE_HARNESS_NAME, SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE,
    SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID, SrplCompileOptimizeSmokeBenchmark,
    run_srpl_compile_optimize_smoke_benchmark,
};
pub use storage_runtime_benchmark::{
    STORAGE_PAGE_STORE_HARNESS_NAME, STORAGE_PAGE_STORE_HARNESS_SOURCE,
    STORAGE_PAGE_STORE_WORKLOAD_ID, StoragePageStoreSmokeBenchmark,
    run_storage_page_store_smoke_benchmark,
};
pub use wal_file_benchmark::{
    RECOVERY_REPLAY_WAL_HARNESS_NAME, RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
    RECOVERY_REPLAY_WAL_WORKLOAD_ID, RecoveryReplayWalSmokeBenchmark, WAL_APPEND_FILE_HARNESS_NAME,
    WAL_APPEND_FILE_HARNESS_SOURCE, WAL_APPEND_FILE_WORKLOAD_ID, WalAppendFileSmokeBenchmark,
    run_recovery_replay_wal_smoke_benchmark, run_wal_append_file_smoke_benchmark,
};
