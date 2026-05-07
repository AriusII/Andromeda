//! Benchmark-side boundary for future `ScenarioEvidence` materialization.
//!
//! This module does not edit or construct catalog `ScenarioEvidence` directly.
//! It records the bounded evidence inputs that a future integration layer can
//! convert into `andromeda-catalog::scenario_evidence::ScenarioEvidence` after
//! resolving the catalog-side `StatsVersion` and `PlanClass` types.
//!
//! Boundary rules:
//!
//! - Evidence is advisory only. `is_authoritative()` always returns `false`.
//! - Duration, sample, and temp budgets are explicit and globally bounded.
//! - Confidence is a fixed `0..=1000` permille value, never a float.
//! - Validity is a half-open timestamp window: `[issued_at, expires_at)`.
//! - Validity is capped by `MAX_EVIDENCE_TTL_MS`.
//! - The target always names a non-zero `ProcedureId`, non-zero
//!   `CatalogVersion`, non-zero `ContractHash`, non-zero stats version, and
//!   one bounded plan class.
//! - Hardware profile, workload class, and measurement provenance are captured
//!   when the source evidence carries those fields.

mod budgets;
mod confidence;
mod context;
mod errors;
mod evidence;
mod json;
mod target;
mod validity;

pub use budgets::BenchmarkEvidenceBudgets;
pub use confidence::BenchmarkEvidenceConfidence;
pub use context::BenchmarkEvidenceContext;
pub use errors::BenchmarkScenarioEvidenceError;
pub use evidence::BenchmarkScenarioEvidence;
pub use target::{BenchmarkPlanClass, BenchmarkScenarioTarget, BenchmarkStatsVersion};
pub use validity::BenchmarkEvidenceValidity;

#[cfg(test)]
mod tests;
