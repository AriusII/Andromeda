use andromeda_time::EngineTimestamp;

use super::{
    EvidenceConfidence, EvidenceScore, ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError,
    ScenarioEvidenceOptimizerBoundary, ScenarioId, ScenarioKind, ScenarioTarget, ValidityWindow,
    digest,
};

/// V0 scored, expirable, advisory scenario evidence record.
///
/// Construct via [`ScenarioEvidence::new`], which performs all bounded
/// validation in a single place. All fields are private so external callers
/// cannot bypass validation by direct struct literal construction.
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
    ///
    /// On success, the resulting record is guaranteed to:
    /// - have a non-zero scenario id, procedure id, catalog version, and stats version,
    /// - carry a score and confidence in `0..=1000`,
    /// - have a validity window with `issued_at < expires_at` and
    ///   `expires_at > 0`,
    /// - carry a non-zero `contract_hash` whenever one is supplied.
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

    /// **Doctrine invariant: scenario evidence is never authoritative.**
    /// This always returns `false`. It exists so optimizer-side code can
    /// express the doctrine at the type level (`assert!(!ev.is_authoritative())`)
    /// without conditional logic.
    pub const fn is_authoritative(&self) -> bool {
        false
    }

    /// **Doctrine invariant: predictive evidence cannot publish stats.**
    /// This always returns `false`; active `StatsVersion` publication requires
    /// typed stats-publication decision evidence in the statistics module, not
    /// ScenarioEvidence alone.
    pub const fn can_drive_active_stats_version_transition(&self) -> bool {
        false
    }

    /// `true` when `now >= expires_at`.
    pub fn is_expired_at(&self, now: EngineTimestamp) -> bool {
        self.validity.is_expired_at(now)
    }

    /// Validate the record for consumption *at* `now`. Returns:
    /// - `Err(NotYetValid)` if the record is issued in the future,
    /// - `Err(Expired)` if the record has expired,
    /// - `Ok(())` otherwise.
    ///
    /// Optimizer code is required to call this (or [`Self::is_expired_at`])
    /// before consuming evidence. Calling neither is a doctrine violation:
    /// there is no silent bypass.
    pub fn validate_for_use_at(&self, now: EngineTimestamp) -> Result<(), ScenarioEvidenceError> {
        if self.validity.is_not_yet_valid_at(now) {
            return Err(ScenarioEvidenceError::NotYetValid);
        }
        if self.validity.is_expired_at(now) {
            return Err(ScenarioEvidenceError::Expired);
        }
        Ok(())
    }

    /// Validate this evidence for advisory consumption at `now`.
    ///
    /// The returned token intentionally has no path to become an authoritative
    /// plan choice. Consumers must combine it with catalog, statistics,
    /// contract, and optimizer decision logic outside this module.
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
