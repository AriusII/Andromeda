//! ScenarioEvidence bridge for bounded plan-cache selection.
//!
//! `ScenarioEvidence` remains advisory. These helpers summarize valid evidence
//! for `andromeda-plan-cache` without letting benchmark output select a plan by
//! itself.

mod advisory_evidence;
mod selection;

pub use advisory_evidence::classify_advisory_evidence_for_key;
pub use selection::select_minimal_plan;
