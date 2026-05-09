//! CRUD workload contracts and scenarios for synthetic diagnostic benchmark execution.
//!
//! This module provides bounded synthetic CRUD workload scaffolding with:
//! - Deterministic data generation (seeded PRNG)
//! - Configurable concurrency and batching
//! - Latency/throughput metrics collection
//! - 6 pre-defined diagnostic scenarios (single/multi-threaded, varying batch sizes)

mod result;

pub use andromeda_bench_workload::{
    CRUD_SCENARIOS, CrudDataGenerator, CrudOperationMetrics, CrudRow, CrudScenarioDefinition,
    MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS, MAX_CRUD_ROWS, MAX_CRUD_THREADS, compute_percentile,
    find_crud_scenario,
};
pub use result::CrudWorkloadResult;

#[cfg(test)]
use andromeda_bench_workload::{
    CRUD_BENCHMARK_BUDGET_ORIGIN, CRUD_BENCHMARK_DECISION_LINKAGE, CRUD_BENCHMARK_PRIMARY_METRIC,
};

#[cfg(test)]
mod tests;
