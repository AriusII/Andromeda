#![forbid(unsafe_code)]

//! Bounded benchmark contracts for Andromeda operational diagnostics.
//!
//! This crate defines benchmark workloads, limits, budgets, and result evidence
//! types. It intentionally does not provide a runtime wire protocol surface.

pub mod audit_file_benchmark;
pub mod btree_benchmark;
pub mod btree_node_codec_benchmark;
pub mod history_store;
pub mod srpl_compiler_benchmark;
pub mod wal_file_benchmark;

mod advisory_boundary;
mod benchmark_history;
mod budget;
mod crud;
mod error;
mod evidence;
mod flat_json;
mod harness;
mod limits;
mod metric_math;
mod regression_detection;
mod request;
mod runner;
mod scenario_boundary;
mod storage_runtime_benchmark;
mod workload;

pub use audit_file_benchmark::{
    AUDIT_APPEND_FILE_SINK_HARNESS_NAME, AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
    AUDIT_APPEND_FILE_SINK_WORKLOAD_ID, AuditAppendFileSinkSmokeBenchmark,
    run_audit_append_file_sink_smoke_benchmark,
};
pub use benchmark_history::{
    BenchmarkHistoryAdvisoryMetadata, BenchmarkHistoryRecord, HistoryQuery, HistoryQueryResult,
    TimeRange,
};

pub use btree_benchmark::{
    BTreeBenchmarkConfig, BTreeBenchmarkContext, BTreeBenchmarkError, benchmark_btree_lookup,
    benchmark_btree_range_scan, setup_btree_lookup_harness, setup_btree_range_scan_harness,
};
pub use btree_node_codec_benchmark::{
    BTREE_NODE_CODEC_HARNESS_NAME, BTREE_NODE_CODEC_HARNESS_SOURCE, BTREE_NODE_CODEC_WORKLOAD_ID,
    BTreeNodeCodecSmokeBenchmark, run_btree_node_codec_smoke_benchmark,
};

pub use budget::{BudgetStatus, PerformanceBudget, evaluate_budget};
pub use crud::{
    CRUD_SCENARIOS, CrudDataGenerator, CrudOperationMetrics, CrudRow, CrudScenarioDefinition,
    CrudWorkloadResult, MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS, MAX_CRUD_ROWS, MAX_CRUD_THREADS,
    compute_percentile, find_crud_scenario,
};
pub use error::BenchmarkError;
pub use evidence::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
    BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER, BenchmarkEvidence,
    BenchmarkMeasurementMode, BenchmarkWorkloadCounter, MAX_BENCHMARK_WORKLOAD_COUNTERS,
};
pub use history_store::BenchmarkHistoryStore;
pub use limits::{
    DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_TEMP_BYTES, DEFAULT_WARMUPS, MAX_DURATION_MS,
    MAX_EVIDENCE_TTL_MS, MAX_SAMPLES, MAX_TEMP_BYTES, MAX_WARMUPS,
};
pub use regression_detection::{
    BenchmarkBaseline, BenchmarkBaselineComparisonError, BenchmarkBaselineContext,
    RegressionAnalysis, RegressionReason,
};
pub use request::{BenchmarkHardwareProfile, BenchmarkRunRequest, validate_run_request};
pub use runner::run_bounded_benchmark;
pub use scenario_boundary::{
    BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence, BenchmarkEvidenceContext,
    BenchmarkEvidenceValidity, BenchmarkPlanClass, BenchmarkScenarioEvidence,
    BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget, BenchmarkStatsVersion,
};
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
pub use workload::{BenchmarkWorkload, BenchmarkWorkloadClass, WORKLOADS, find_workload};
