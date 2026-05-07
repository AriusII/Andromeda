use super::{EvidenceConfidence, EvidenceScore, ScenarioId, ScenarioTarget};

/// Closed optimizer consumption boundary for [`super::ScenarioEvidence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScenarioEvidenceOptimizerBoundary {
    AdvisoryOnly,
}

impl ScenarioEvidenceOptimizerBoundary {
    pub const VARIANT_COUNT: usize = 1;

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::AdvisoryOnly => 0x01,
        }
    }

    pub const fn is_authoritative(self) -> bool {
        false
    }

    pub const fn can_select_plan_alone(self) -> bool {
        false
    }
}

/// Validated, non-authoritative ScenarioEvidence consumption token.
///
/// Constructed only through [`super::ScenarioEvidence::advisory_use_at`], which
/// forces callers to check the validity window before the evidence can be
/// handed to a future optimizer path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioEvidenceAdvisoryUse {
    scenario_id: ScenarioId,
    digest: [u8; 32],
    boundary: ScenarioEvidenceOptimizerBoundary,
    target: ScenarioTarget,
    score: EvidenceScore,
    confidence: EvidenceConfidence,
}

impl ScenarioEvidenceAdvisoryUse {
    pub(super) const fn new(
        scenario_id: ScenarioId,
        digest: [u8; 32],
        boundary: ScenarioEvidenceOptimizerBoundary,
        target: ScenarioTarget,
        score: EvidenceScore,
        confidence: EvidenceConfidence,
    ) -> Self {
        Self {
            scenario_id,
            digest,
            boundary,
            target,
            score,
            confidence,
        }
    }

    pub const fn scenario_id(self) -> ScenarioId {
        self.scenario_id
    }

    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    pub const fn boundary(self) -> ScenarioEvidenceOptimizerBoundary {
        self.boundary
    }

    pub const fn target(self) -> ScenarioTarget {
        self.target
    }

    pub const fn score(self) -> EvidenceScore {
        self.score
    }

    pub const fn confidence(self) -> EvidenceConfidence {
        self.confidence
    }

    pub const fn is_authoritative(self) -> bool {
        self.boundary.is_authoritative()
    }

    pub const fn can_select_plan_alone(self) -> bool {
        self.boundary.can_select_plan_alone()
    }

    pub const fn can_drive_active_stats_version_transition(self) -> bool {
        false
    }
}
