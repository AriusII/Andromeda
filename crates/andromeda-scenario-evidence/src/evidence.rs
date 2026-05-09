pub const BENCHMARK_EVIDENCE_AUTHORITATIVE: bool = false;
pub const BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE: bool = false;
pub const BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY: &str = "advisory-only";
pub const BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER: &str =
    "deterministic-run-clock-placeholder";
pub const MAX_BENCHMARK_WORKLOAD_COUNTERS: usize = 8;
pub const MAX_BENCHMARK_WORKLOAD_COUNTER_NAME_BYTES: usize = 64;
pub const MAX_BENCHMARK_WORKLOAD_COUNTER_UNIT_BYTES: usize = 32;

mod counter;
mod measurement;
mod model;

pub use counter::BenchmarkWorkloadCounter;
pub use measurement::BenchmarkMeasurementMode;
pub use model::BenchmarkEvidence;

#[cfg(test)]
mod tests;
