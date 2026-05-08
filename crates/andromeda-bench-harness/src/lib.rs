#![forbid(unsafe_code)]

//! Future owner crate scaffold for bounded Andromeda benchmark harnesses.
//!
//! This crate intentionally defines no public API yet. Benchmark execution stays
//! in `andromeda-bench` until a later extraction registers this package in the
//! workspace and proves compatibility.
//!
//! Ownership constraints:
//! - Harness output is advisory diagnostic evidence, not truth.
//! - Runs must be bounded by workload identity, shape version, duration,
//!   samples, warmups, and temporary bytes.
//! - Benchmark execution and GPU work must stay outside C5 commit, WAL,
//!   rollback, recovery, MVCC short-visibility, catalog publication, and
//!   security-critical paths.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.
