#![forbid(unsafe_code)]

//! Future owner crate scaffold for advisory Andromeda batch analytics.
//!
//! This crate intentionally defines no public API yet. Analytics behavior stays
//! in current owners until a later extraction registers this package in the
//! workspace and proves compatibility.
//!
//! Ownership constraints:
//! - Analytical output is advisory until a runtime owner validates and publishes
//!   a version-bound result.
//! - Analytics must not enter C5 commit, WAL, rollback, recovery, MVCC
//!   short-visibility, catalog publication, or security-critical paths.
//! - GPU acceleration must be optional, disableable, and backed by CPU fallback.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.
