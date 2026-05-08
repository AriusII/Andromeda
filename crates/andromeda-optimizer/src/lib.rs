#![forbid(unsafe_code)]

//! Future owner crate scaffold for bounded Andromeda optimizer behavior.
//!
//! This crate intentionally defines no public API yet. Optimizer behavior stays
//! in the current broad owners until a later extraction registers this package
//! in the workspace and proves compatibility.
//!
//! Ownership constraints:
//! - Plan choice must be bounded, versioned, observable, explainable, and
//!   disableable.
//! - ScenarioEvidence and benchmark output are advisory and cannot select a
//!   plan alone.
//! - Every accepted or rejected adaptive input must be explainable through
//!   DecisionTrace.
//! - GPU and analytics work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
