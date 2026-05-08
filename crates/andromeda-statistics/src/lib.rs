#![forbid(unsafe_code)]

//! Future owner crate scaffold for versioned Andromeda statistics.
//!
//! This crate intentionally defines no public API yet. Statistics behavior
//! remains in the current broad owners until a later extraction registers this
//! package in the workspace and moves code with compatibility tests.
//!
//! Ownership constraints:
//! - Statistics evidence must be version-bound and advisory until validated.
//! - Benchmark output, GPU output, RAM state, and temporary files are not truth.
//! - GPU-assisted refresh work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
//! - Optimizer consumers must explain accepted and rejected statistics through
//!   DecisionTrace.
