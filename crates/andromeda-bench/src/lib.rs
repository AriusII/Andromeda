#![forbid(unsafe_code)]

//! Bounded benchmark contracts for Andromeda operational diagnostics.
//!
//! This crate defines benchmark workloads, limits, budgets, and result evidence
//! types. It intentionally does not provide a runtime wire protocol surface.

pub mod benchmark_history;
pub mod btree_benchmark;
pub mod history_store;
pub mod regression_detection;

mod budget;
mod crud;
mod error;
mod evidence;
mod flat_json;
mod limits;
mod request;
mod runner;
mod workload;

#[cfg(test)]
mod cross_commit_history_integration_tests;
#[cfg(test)]
mod regression_detection_integration_tests;

pub use benchmark_history::{BenchmarkHistoryRecord, HistoryQuery, HistoryQueryResult, TimeRange};

pub use btree_benchmark::{
    BTreeBenchmarkConfig, BTreeBenchmarkContext, BTreeBenchmarkError, benchmark_btree_lookup,
    benchmark_btree_range_scan, setup_btree_lookup_harness, setup_btree_range_scan_harness,
};

pub use budget::{BudgetStatus, PerformanceBudget, evaluate_budget};
pub use crud::{
    CRUD_SCENARIOS, CrudDataGenerator, CrudOperationMetrics, CrudRow, CrudScenarioDefinition,
    CrudWorkloadResult, compute_percentile, find_crud_scenario,
};
pub use error::BenchmarkError;
pub use evidence::BenchmarkEvidence;
pub use history_store::BenchmarkHistoryStore;
pub use limits::{
    DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_SAMPLES,
    MAX_WARMUPS,
};
pub use regression_detection::{BenchmarkBaseline, RegressionAnalysis, RegressionReason};
pub use request::{BenchmarkHardwareProfile, BenchmarkRunRequest, validate_run_request};
pub use runner::run_bounded_benchmark;
pub use workload::{BenchmarkWorkload, WORKLOADS, find_workload};
