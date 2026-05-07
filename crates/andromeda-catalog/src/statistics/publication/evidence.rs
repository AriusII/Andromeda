use andromeda_observe::TraceId;

use crate::scenario_evidence::{ScenarioEvidenceAdvisoryUse, ScenarioId, ScenarioTarget};

use super::error::StatsPublicationSwitchError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPublicationDecisionEvidenceKind {
    StatsBuilder,
    CanonicalValidation,
    OperatorReview,
    RecoveryReview,
    PredictiveScenarioEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatsPublicationAdvisoryEvidenceReference {
    scenario_id: ScenarioId,
    digest: [u8; 32],
    target: ScenarioTarget,
}

impl StatsPublicationAdvisoryEvidenceReference {
    pub fn new(
        scenario_id: ScenarioId,
        digest: [u8; 32],
        target: ScenarioTarget,
    ) -> Result<Self, StatsPublicationSwitchError> {
        if digest == [0u8; 32] {
            return Err(StatsPublicationSwitchError::AdvisoryEvidenceDigestZero);
        }
        target
            .validate()
            .map_err(StatsPublicationSwitchError::AdvisoryEvidenceTargetRejected)?;
        Ok(Self {
            scenario_id,
            digest,
            target,
        })
    }

    pub(super) fn from_advisory_use(
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        if advisory_use.can_drive_active_stats_version_transition() {
            return Err(StatsPublicationSwitchError::AdvisoryEvidenceCannotDriveActiveStatsVersion);
        }
        Self::new(
            advisory_use.scenario_id(),
            advisory_use.digest(),
            advisory_use.target(),
        )
    }

    pub const fn scenario_id(self) -> ScenarioId {
        self.scenario_id
    }

    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    pub const fn target(self) -> ScenarioTarget {
        self.target
    }

    pub(super) fn matches_advisory_use(self, advisory_use: ScenarioEvidenceAdvisoryUse) -> bool {
        self.scenario_id == advisory_use.scenario_id()
            && self.digest == advisory_use.digest()
            && self.target == advisory_use.target()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsPublicationDecisionEvidence {
    kind: StatsPublicationDecisionEvidenceKind,
    trace_id: TraceId,
    advisory_evidence: Option<StatsPublicationAdvisoryEvidenceReference>,
}

impl StatsPublicationDecisionEvidence {
    pub fn new(
        kind: StatsPublicationDecisionEvidenceKind,
        trace_id: TraceId,
    ) -> Result<Self, StatsPublicationSwitchError> {
        if trace_id.is_zero() {
            return Err(StatsPublicationSwitchError::DecisionEvidenceTraceIdZero);
        }
        if matches!(
            kind,
            StatsPublicationDecisionEvidenceKind::PredictiveScenarioEvidence
        ) {
            return Err(StatsPublicationSwitchError::PredictiveEvidenceRequiresAdvisoryIdentity);
        }
        Ok(Self {
            kind,
            trace_id,
            advisory_evidence: None,
        })
    }

    pub(crate) fn new_with_advisory_reference(
        kind: StatsPublicationDecisionEvidenceKind,
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        let advisory_reference =
            StatsPublicationAdvisoryEvidenceReference::from_advisory_use(advisory_use)?;
        Self::new_with_advisory_identity(kind, trace_id, advisory_use, advisory_reference)
    }

    pub(crate) fn new_with_advisory_identity(
        kind: StatsPublicationDecisionEvidenceKind,
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
        advisory_identity: StatsPublicationAdvisoryEvidenceReference,
    ) -> Result<Self, StatsPublicationSwitchError> {
        if trace_id.is_zero() {
            return Err(StatsPublicationSwitchError::DecisionEvidenceTraceIdZero);
        }
        if !advisory_identity.matches_advisory_use(advisory_use) {
            return Err(StatsPublicationSwitchError::AdvisoryEvidenceIdentityMismatch);
        }
        Ok(Self {
            kind,
            trace_id,
            advisory_evidence: Some(advisory_identity),
        })
    }

    pub fn stats_builder(trace_id: TraceId) -> Result<Self, StatsPublicationSwitchError> {
        Self::new(StatsPublicationDecisionEvidenceKind::StatsBuilder, trace_id)
    }

    pub fn canonical_validation(trace_id: TraceId) -> Result<Self, StatsPublicationSwitchError> {
        Self::new(
            StatsPublicationDecisionEvidenceKind::CanonicalValidation,
            trace_id,
        )
    }

    pub fn canonical_validation_with_advisory_reference(
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        Self::new_with_advisory_reference(
            StatsPublicationDecisionEvidenceKind::CanonicalValidation,
            trace_id,
            advisory_use,
        )
    }

    pub fn canonical_validation_with_advisory_identity(
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
        advisory_identity: StatsPublicationAdvisoryEvidenceReference,
    ) -> Result<Self, StatsPublicationSwitchError> {
        Self::new_with_advisory_identity(
            StatsPublicationDecisionEvidenceKind::CanonicalValidation,
            trace_id,
            advisory_use,
            advisory_identity,
        )
    }

    pub fn operator_review(trace_id: TraceId) -> Result<Self, StatsPublicationSwitchError> {
        Self::new(
            StatsPublicationDecisionEvidenceKind::OperatorReview,
            trace_id,
        )
    }

    pub fn recovery_review(trace_id: TraceId) -> Result<Self, StatsPublicationSwitchError> {
        Self::new(
            StatsPublicationDecisionEvidenceKind::RecoveryReview,
            trace_id,
        )
    }

    pub fn recovery_review_with_advisory_reference(
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        Self::new_with_advisory_reference(
            StatsPublicationDecisionEvidenceKind::RecoveryReview,
            trace_id,
            advisory_use,
        )
    }

    pub fn predictive_scenario_evidence(
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        Self::new_with_advisory_reference(
            StatsPublicationDecisionEvidenceKind::PredictiveScenarioEvidence,
            trace_id,
            advisory_use,
        )
    }

    pub const fn kind(self) -> StatsPublicationDecisionEvidenceKind {
        self.kind
    }

    pub const fn trace_id(self) -> TraceId {
        self.trace_id
    }

    pub const fn advisory_evidence_reference(
        self,
    ) -> Option<StatsPublicationAdvisoryEvidenceReference> {
        self.advisory_evidence
    }

    pub const fn is_predictive_only(self) -> bool {
        matches!(
            self.kind,
            StatsPublicationDecisionEvidenceKind::PredictiveScenarioEvidence
        )
    }

    pub const fn can_drive_active_stats_version_transition(self) -> bool {
        self.kind.can_drive_active_stats_version_transition()
    }

    pub const fn advisory_evidence_can_drive_active_stats_version_transition(self) -> bool {
        false
    }
}

impl StatsPublicationDecisionEvidenceKind {
    const fn can_drive_active_stats_version_transition(self) -> bool {
        !matches!(self, Self::PredictiveScenarioEvidence)
    }
}
