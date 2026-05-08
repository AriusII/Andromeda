#![forbid(unsafe_code)]

//! Future owner crate scaffold for advisory Andromeda regression analysis.
//!
//! This crate intentionally defines no public API yet. Regression behavior stays
//! in `andromeda-bench` until a later extraction registers this package in the
//! workspace and proves compatibility.
//!
//! Ownership constraints:
//! - Regression reports are diagnostic and advisory, not storage, catalog, WAL,
//!   recovery, optimizer, or security truth.
//! - Comparisons must be version-bound and reject incompatible workloads,
//!   baselines, hardware profiles, statistics, policies, and plan classes.
//! - Benchmark output cannot select plans or publish statistics by itself.
//! - Benchmark, analytics, and GPU work must stay outside C5 commit, WAL,
//!   rollback, recovery, MVCC short-visibility, catalog publication, and
//!   security-critical paths.
