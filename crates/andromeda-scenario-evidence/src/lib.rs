#![forbid(unsafe_code)]

//! Future owner crate scaffold for advisory Andromeda ScenarioEvidence.
//!
//! This crate intentionally defines no public API yet. ScenarioEvidence behavior
//! stays in current owners until a later extraction registers this package in
//! the workspace and proves compatibility.
//!
//! Ownership constraints:
//! - ScenarioEvidence is advisory, expirable, version-bound, and disableable.
//! - Benchmark output cannot become trusted evidence without target, validity,
//!   confidence, budget, and measurement-mode checks.
//! - Evidence cannot select a plan, publish statistics, or bypass Procedure,
//!   WAL, recovery, protocol, or security validation by itself.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.
