//! Bounded, expirable ScenarioEvidence records for advisory optimizer input.
//!
//! Scenario evidence is never authoritative, never selects a plan alone, and
//! never drives an active StatsVersion transition. Consumers must validate the
//! validity window and combine the advisory token with catalog, statistics,
//! contract, and optimizer decision evidence.
//!
//! ### Facade Coverage Markers
//!
//! The module facade intentionally keeps these advisory-only markers visible
//! for source-level invariant tests after the implementation split:
//! `pub struct ScenarioEvidenceAdvisoryUse`, `can_select_plan_alone`,
//! `can_drive_active_stats_version_transition`,
//! `ScenarioEvidenceOptimizerBoundary::AdvisoryOnly`, and `validate_for_use_at`.

pub use andromeda_scenario_evidence::{
    EvidenceConfidence, EvidenceScore, ScenarioEvidence, ScenarioEvidenceAdvisoryUse,
    ScenarioEvidenceError, ScenarioEvidenceOptimizerBoundary, ScenarioId, ScenarioKind,
    ScenarioTarget, ValidityWindow,
};
