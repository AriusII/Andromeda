#![forbid(unsafe_code)]

//! Bounded Andromeda benchmark workload contracts.
//!
//! This crate owns workload identity, request limits, budget evaluation, and
//! bounded diagnostic metadata. It does not execute benchmarks and benchmark
//! output remains advisory only.

mod budget;
mod crud;
mod error;
mod limits;
mod metric_math;
mod request;
mod workload;

pub use budget::{BudgetStatus, PerformanceBudget, evaluate_budget};
pub use crud::{
    CRUD_BENCHMARK_BUDGET_ORIGIN, CRUD_BENCHMARK_DECISION_LINKAGE, CRUD_BENCHMARK_PRIMARY_METRIC,
    CRUD_SCENARIOS, CrudDataGenerator, CrudOperationMetrics, CrudRow, CrudScenarioDefinition,
    MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS, MAX_CRUD_ROWS, MAX_CRUD_THREADS, compute_percentile,
    find_crud_scenario,
};
pub use error::BenchmarkError;
pub use limits::{
    DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_TEMP_BYTES, DEFAULT_WARMUPS, MAX_DURATION_MS,
    MAX_EVIDENCE_TTL_MS, MAX_SAMPLES, MAX_TEMP_BYTES, MAX_WARMUPS,
};
pub use metric_math::{error_rate_ppm, percent_change};
pub use request::{BenchmarkHardwareProfile, BenchmarkRunRequest, validate_run_request};
pub use workload::{
    AUDIT_APPEND_FILE_SINK_SMOKE_WORKLOAD_ID, BENCHMARK_BUDGET_ORIGIN, BENCHMARK_DECISION_LINKAGE,
    BENCHMARK_PRIMARY_METRIC, BTREE_LOOKUP_SMOKE_WORKLOAD_ID, BTREE_NODE_CODEC_SMOKE_WORKLOAD_ID,
    BTREE_RANGE_SCAN_SMOKE_WORKLOAD_ID, BenchmarkWorkload, BenchmarkWorkloadClass,
    PROTOCOL_SMOKE_CONTRACT_WORKLOAD_ID, RECOVERY_REPLAY_WAL_SMOKE_WORKLOAD_ID,
    SRPL_COMPILE_OPTIMIZE_SMOKE_WORKLOAD_ID, STORAGE_PAGE_STORE_SMOKE_WORKLOAD_ID,
    VERTICAL_V0_SMOKE_WORKLOAD_ID, WAL_APPEND_FILE_SMOKE_WORKLOAD_ID, WAL_APPEND_SMOKE_WORKLOAD_ID,
    WORKLOADS, find_workload,
};
