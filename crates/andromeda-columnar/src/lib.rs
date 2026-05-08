#![forbid(unsafe_code)]

//! Future owner crate scaffold for Andromeda columnar analytical descriptors.
//!
//! This crate intentionally defines no public API yet. Columnar behavior stays
//! in current owners until a later extraction registers this package in the
//! workspace and proves compatibility.
//!
//! Ownership constraints:
//! - Columnar artifacts are derived and advisory unless a later C5 design adds
//!   explicit codecs, WAL coverage, and crash/recovery validation.
//! - Benchmark output, GPU output, RAM state, and temporary files are not truth.
//! - GPU and analytics work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.
