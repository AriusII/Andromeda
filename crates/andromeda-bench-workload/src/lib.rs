#![forbid(unsafe_code)]

//! Future owner crate scaffold for bounded Andromeda benchmark workloads.
//!
//! This crate intentionally defines no public API yet. Workload behavior stays
//! in `andromeda-bench` until a later extraction registers this package in the
//! workspace and proves compatibility.
//!
//! Ownership constraints:
//! - Workloads must have explicit identity, shape version, limits, and stop
//!   rules.
//! - Benchmark output is advisory and cannot become storage, catalog, optimizer,
//!   recovery, or security truth.
//! - Benchmark and GPU work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.
