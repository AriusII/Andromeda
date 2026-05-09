//! CRUD workload contracts and scenario definitions.

mod data;
mod metrics;
mod scenario;

pub use data::{CrudDataGenerator, CrudRow};
pub use metrics::{CrudOperationMetrics, compute_percentile};
pub use scenario::{
    CRUD_BENCHMARK_BUDGET_ORIGIN, CRUD_BENCHMARK_DECISION_LINKAGE, CRUD_BENCHMARK_PRIMARY_METRIC,
    CRUD_SCENARIOS, CrudScenarioDefinition, MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS,
    MAX_CRUD_ROWS, MAX_CRUD_THREADS, find_crud_scenario,
};
