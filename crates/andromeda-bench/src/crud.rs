//! CRUD workload contracts and scenarios for synthetic diagnostic benchmark execution.
//!
//! This module provides bounded synthetic CRUD workload scaffolding with:
//! - Deterministic data generation (seeded PRNG)
//! - Configurable concurrency and batching
//! - Latency/throughput metrics collection
//! - 6 pre-defined diagnostic scenarios (single/multi-threaded, varying batch sizes)

mod data;
mod metrics;
mod result;
mod scenario;

pub use data::{CrudDataGenerator, CrudRow};
pub use metrics::{CrudOperationMetrics, compute_percentile};
pub use result::CrudWorkloadResult;
pub use scenario::{
    CRUD_SCENARIOS, CrudScenarioDefinition, MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS,
    MAX_CRUD_ROWS, MAX_CRUD_THREADS, find_crud_scenario,
};

#[cfg(test)]
use scenario::{
    CRUD_BENCHMARK_BUDGET_ORIGIN, CRUD_BENCHMARK_DECISION_LINKAGE, CRUD_BENCHMARK_PRIMARY_METRIC,
};

#[cfg(test)]
mod tests;
