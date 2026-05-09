#![forbid(unsafe_code)]

//! Benchmark regression detection.
//!
//! This module implements performance regression detection for benchmarks.
//! It compares current evidence against baseline evidence and determines:
//! 1. If any metric exceeded its budget (absolute failure)
//! 2. If any metric degraded vs. baseline (relative regression)
//!
//!
//! Operational integration note: this module owns the comparison logic and
//! baseline serialization. Artifact storage and CI entrypoints should call this
//! crate directly instead of routing through benchmark compatibility crates.

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
