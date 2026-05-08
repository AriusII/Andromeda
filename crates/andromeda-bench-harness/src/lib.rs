#![forbid(unsafe_code)]

//! Bounded helpers for Andromeda benchmark harness execution.
//!
//! This crate owns reusable harness support only. Specific engine benchmark
//! smoke runs stay in their engine-facing crate and remain advisory diagnostics.

mod temp;

pub use self::temp::{BenchmarkTempDir, BenchmarkTempFile, elapsed_micros};
