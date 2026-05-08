use andromeda_observe::TraceId;
use andromeda_plan_cache::{
    PLAN_SELECTION_MAX_CANDIDATES, PlanCacheKey, PlanCandidate, PlanSelectionError,
    PlanSelectionOutcome, select_minimal_plan_with_advisory_evidence,
};
use andromeda_time::EngineTimestamp;

use crate::scenario_evidence::ScenarioEvidence;

use super::advisory_evidence::summarize_advisory_evidence;

/// Select the lowest-ranked candidate that matches the key's plan class.
///
/// ScenarioEvidence is validated, summarized, and traced, but it never
/// overrides the bounded static rank.  Ties are broken by `PlanCandidateId`
/// for deterministic cross-node behavior.
pub fn select_minimal_plan(
    key: PlanCacheKey,
    candidates: &[PlanCandidate],
    scenario_evidence: &[ScenarioEvidence],
    now: EngineTimestamp,
    trace_id: TraceId,
) -> Result<PlanSelectionOutcome, PlanSelectionError> {
    if trace_id.is_zero() {
        return Err(PlanSelectionError::TraceIdZero);
    }
    if candidates.is_empty() {
        return Err(PlanSelectionError::NoCandidates);
    }
    if candidates.len() > PLAN_SELECTION_MAX_CANDIDATES {
        return Err(PlanSelectionError::TooManyCandidates);
    }

    let advisory_evidence = summarize_advisory_evidence(&key, scenario_evidence, now)?;
    select_minimal_plan_with_advisory_evidence(key, candidates, advisory_evidence, trace_id)
}
