use andromeda_observe::TraceId;
use andromeda_time::EngineTimestamp;

use crate::scenario_evidence::ScenarioEvidence;

use super::advisory_evidence::summarize_advisory_evidence;
use super::decision::PlanDecisionReasonCode;
use super::{
    PLAN_SELECTION_MAX_CANDIDATES, PlanCacheKey, PlanClass, PlanDecisionEvidence,
    PlanDecisionOutcome,
};

/// Stable opaque id for a candidate physical plan.
///
/// The catalog gate does not own executable plan state.  This id is an
/// external handle supplied by a compiler or future optimizer layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanCandidateId(u64);

impl PlanCandidateId {
    /// Construct a non-zero candidate id.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Deterministic static candidate rank.
///
/// Lower values are preferred.  The rank is intentionally separate from
/// [`ScenarioEvidence`]: benchmark evidence can be recorded and summarized,
/// but it cannot become the sole authority that selects a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanCandidateRank(u16);

impl PlanCandidateRank {
    pub const MAX_RAW: u16 = 1_000;

    pub const fn from_permille(value: u16) -> Result<Self, PlanSelectionError> {
        if value > Self::MAX_RAW {
            Err(PlanSelectionError::CandidateRankOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

/// Opaque candidate considered by the minimal plan selector.
///
/// The candidate carries only a class, a static rank, and a digest over the
/// external plan representation.  No SRPL body, plan text, SQL text, or
/// executable state enters the catalog cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCandidate {
    plan_id: PlanCandidateId,
    plan_class: PlanClass,
    static_rank: PlanCandidateRank,
    plan_digest: [u8; 32],
}

impl PlanCandidate {
    pub fn new(
        plan_id: PlanCandidateId,
        plan_class: PlanClass,
        static_rank: PlanCandidateRank,
        plan_digest: [u8; 32],
    ) -> Result<Self, PlanSelectionError> {
        if plan_digest == [0; 32] {
            return Err(PlanSelectionError::PlanDigestZero);
        }
        Ok(Self {
            plan_id,
            plan_class,
            static_rank,
            plan_digest,
        })
    }

    pub const fn plan_id(self) -> PlanCandidateId {
        self.plan_id
    }

    pub const fn plan_class(self) -> PlanClass {
        self.plan_class
    }

    pub const fn static_rank(self) -> PlanCandidateRank {
        self.static_rank
    }

    pub const fn plan_digest(self) -> [u8; 32] {
        self.plan_digest
    }
}

/// Result of deterministic minimal plan selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSelectionOutcome {
    key: PlanCacheKey,
    selected: PlanCandidate,
    trace: PlanDecisionEvidence,
}

impl PlanSelectionOutcome {
    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn selected(&self) -> PlanCandidate {
        self.selected
    }

    pub const fn trace(&self) -> &PlanDecisionEvidence {
        &self.trace
    }
}

/// Select the lowest-ranked candidate that matches the key's [`PlanClass`].
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
    let mut selected: Option<PlanCandidate> = None;
    let mut matching_candidate_count = 0u8;

    for candidate in candidates {
        if candidate.plan_class() != key.plan_class {
            continue;
        }
        matching_candidate_count = matching_candidate_count.saturating_add(1);
        let should_replace = match selected {
            None => true,
            Some(current) => {
                (candidate.static_rank(), candidate.plan_id())
                    < (current.static_rank(), current.plan_id())
            }
        };
        if should_replace {
            selected = Some(*candidate);
        }
    }

    let selected = selected.ok_or(PlanSelectionError::NoCandidateForPlanClass)?;
    let trace = PlanDecisionEvidence::new(
        trace_id,
        key,
        PlanDecisionOutcome::Selected,
        PlanDecisionReasonCode::MinimalRankSelected,
        Some(selected.plan_id()),
        candidates.len() as u8,
        matching_candidate_count,
        advisory_evidence,
    );

    Ok(PlanSelectionOutcome {
        key,
        selected,
        trace,
    })
}

/// Errors raised by the minimal plan selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSelectionError {
    TraceIdZero,
    CandidateRankOutOfRange,
    PlanDigestZero,
    NoCandidates,
    TooManyCandidates,
    TooManyScenarioEvidence,
    NoCandidateForPlanClass,
}

impl core::fmt::Display for PlanSelectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanSelectionError::TraceIdZero => {
                f.write_str("plan selection requires a non-zero TraceId")
            }
            PlanSelectionError::CandidateRankOutOfRange => {
                f.write_str("PlanCandidateRank raw value must be in 0..=1000")
            }
            PlanSelectionError::PlanDigestZero => {
                f.write_str("PlanCandidate.plan_digest must be non-zero")
            }
            PlanSelectionError::NoCandidates => {
                f.write_str("plan selection requires at least one candidate")
            }
            PlanSelectionError::TooManyCandidates => {
                f.write_str("plan selection candidate count exceeds the bounded maximum")
            }
            PlanSelectionError::TooManyScenarioEvidence => {
                f.write_str("scenario evidence count exceeds the bounded maximum")
            }
            PlanSelectionError::NoCandidateForPlanClass => {
                f.write_str("no candidate matched the requested PlanClass")
            }
        }
    }
}
