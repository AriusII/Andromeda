use andromeda_time::EngineTimestamp;

use super::{
    EvidenceConfidence, EvidenceScore, ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError,
    ScenarioEvidenceOptimizerBoundary, ScenarioId, ScenarioKind, ScenarioTarget, ValidityWindow,
    digest,
};

/// V0 scored, expirable, advisory scenario evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioEvidence {
    scenario_id: ScenarioId,
    kind: ScenarioKind,
    target: ScenarioTarget,
    score: EvidenceScore,
    confidence: EvidenceConfidence,
    validity: ValidityWindow,
}

impl ScenarioEvidence {
    /// Construct an evidence record after running every bounded check.
    pub fn new(
        scenario_id: ScenarioId,
        kind: ScenarioKind,
        target: ScenarioTarget,
        score: EvidenceScore,
        confidence: EvidenceConfidence,
        validity: ValidityWindow,
    ) -> Result<Self, ScenarioEvidenceError> {
        target.validate()?;
        Ok(Self {
            scenario_id,
            kind,
            target,
            score,
            confidence,
            validity,
        })
    }

    pub const fn scenario_id(&self) -> ScenarioId {
        self.scenario_id
    }

    pub const fn kind(&self) -> ScenarioKind {
        self.kind
    }

    pub const fn target(&self) -> &ScenarioTarget {
        &self.target
    }

    pub const fn score(&self) -> EvidenceScore {
        self.score
    }

    pub const fn confidence(&self) -> EvidenceConfidence {
        self.confidence
    }

    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }

    pub const fn optimizer_boundary(&self) -> ScenarioEvidenceOptimizerBoundary {
        ScenarioEvidenceOptimizerBoundary::AdvisoryOnly
    }

    pub const fn can_select_plan_alone(&self) -> bool {
        self.optimizer_boundary().can_select_plan_alone()
    }

    pub const fn is_authoritative(&self) -> bool {
        false
    }

    pub const fn can_drive_active_stats_version_transition(&self) -> bool {
        false
    }

    pub fn is_expired_at(&self, now: EngineTimestamp) -> bool {
        self.validity.is_expired_at(now)
    }

    pub fn validate_for_use_at(&self, now: EngineTimestamp) -> Result<(), ScenarioEvidenceError> {
        if self.validity.is_not_yet_valid_at(now) {
            return Err(ScenarioEvidenceError::NotYetValid);
        }
        if self.validity.is_expired_at(now) {
            return Err(ScenarioEvidenceError::Expired);
        }
        Ok(())
    }

    pub fn advisory_use_at(
        &self,
        now: EngineTimestamp,
    ) -> Result<ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError> {
        self.validate_for_use_at(now)?;
        Ok(ScenarioEvidenceAdvisoryUse::new(
            self.scenario_id,
            self.digest(),
            self.optimizer_boundary(),
            self.target,
            self.score,
            self.confidence,
        ))
    }

    /// Compute a deterministic 32-byte digest over every field.
    pub fn digest(&self) -> [u8; 32] {
        digest::compute_digest(self)
    }
}
