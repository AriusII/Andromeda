#![forbid(unsafe_code)]

//! Benchmark regression detection.
//!
//! This module implements performance regression detection for benchmarks.
//! It compares current evidence against baseline evidence and determines:
//! 1. If any metric exceeded its budget (absolute failure)
//! 2. If any metric degraded vs. baseline (relative regression)
//!
//! TECH-DEBT:
//! - Context: this module can serialize/deserialize baselines, but CI artifact
//!   load/store and regression-gate wiring are not implemented in Rust yet.
//! - Risk: regressions are detected only when this module is invoked manually.
//! - Closure: wire baseline artifact I/O and call the analysis path from the
//!   performance workflow entrypoint.

mod advisory_json;
mod baseline;
mod baseline_context;
mod comparison;
mod errors;
mod mismatch_rejection;
mod reason;
mod threshold_evaluation;

#[cfg(test)]
mod tests;

pub use baseline::BenchmarkBaseline;
pub use baseline_context::BenchmarkBaselineContext;
pub use comparison::RegressionAnalysis;
pub use errors::BenchmarkBaselineComparisonError;
pub use reason::RegressionReason;
