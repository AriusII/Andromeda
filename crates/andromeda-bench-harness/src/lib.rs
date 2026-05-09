#![forbid(unsafe_code)]

//! Bounded helpers for Andromeda benchmark harness execution.
//!
//! This crate owns reusable harness support only. Specific engine benchmark
//! smoke runs stay in their engine-facing crate and remain advisory diagnostics.

mod runner;
mod temp;

pub use self::runner::{
    LatencyEvidence, SYNTHETIC_LATENCY_SOURCE, SYNTHETIC_MODEL_VERSION, counter_from_usize,
    counters_from_usize, harness_latency_evidence, requested_sample_counters,
    run_bounded_benchmark_with_latency_dispatch, synthetic_latency_evidence,
    validate_workload_counters,
};
pub use self::temp::{BenchmarkTempDir, BenchmarkTempFile, elapsed_micros};
