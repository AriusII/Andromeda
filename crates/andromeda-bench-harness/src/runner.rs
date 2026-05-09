//! Shared benchmark runner evidence helpers.

mod bounded;
mod counters;
mod latency;
mod synthetic;

pub use bounded::run_bounded_benchmark_with_latency_dispatch;
pub use counters::{
    counter_from_usize, counters_from_usize, requested_sample_counters, validate_workload_counters,
};
pub use latency::{LatencyEvidence, harness_latency_evidence};
pub use synthetic::{
    SYNTHETIC_LATENCY_SOURCE, SYNTHETIC_MODEL_VERSION, synthetic_latency_evidence,
};
