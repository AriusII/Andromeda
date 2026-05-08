#![forbid(unsafe_code)]

//! Future owner crate scaffold for Andromeda plan cache identity and policy.
//!
//! This crate intentionally defines no public API yet. Plan cache behavior
//! remains in the current broad owners until a later extraction registers this
//! package in the workspace and moves code with compatibility tests.
//!
//! Ownership constraints:
//! - Plan entries must be bound to complete catalog, contract, statistics,
//!   policy, and plan-class identity.
//! - Cache output is not durable truth and must be bounded and disableable.
//! - Benchmark output and ScenarioEvidence are advisory and must be traceable
//!   when used or ignored.
//! - GPU and analytics work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
