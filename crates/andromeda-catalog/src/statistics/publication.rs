use andromeda_core::CatalogVersion;
use andromeda_observe::TraceId;

use crate::{
    contracts::StatsVersion,
    digest::{Sha256, digest_prefix_hex},
    scenario_evidence::{
        ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError, ScenarioId, ScenarioTarget,
    },
};

use super::{
    HistogramPlaceholder, MAX_BUCKETS_PER_HISTOGRAM, MAX_HISTOGRAMS_PER_PUBLICATION,
    STATS_PUBLICATION_DOMAIN, StatsColumnTarget, StatsPublicationDigest, StatsValidationError,
};

/// Maximum number of switch traces retained by a publication switch.
pub const STATS_PUBLICATION_SWITCH_HISTORY_LIMIT: usize = 128;

/// Maximum UTF-8 byte length of the operator-supplied switch reason.
pub const STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsPublicationSummary {
    pub catalog_version: CatalogVersion,
    pub version: StatsVersion,
    pub digest: StatsPublicationDigest,
    pub entry_count: usize,
}

impl StatsPublicationSummary {
    pub fn from_publication(publication: &StatsPublication) -> Self {
        Self {
            catalog_version: publication.catalog_version,
            version: publication.version,
            digest: publication.digest,
            entry_count: publication.entries.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsPublication {
    catalog_version: CatalogVersion,
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
    digest: StatsPublicationDigest,
}

impl StatsPublication {
    pub fn catalog_version(&self) -> CatalogVersion {
        self.catalog_version
    }

    pub fn version(&self) -> StatsVersion {
        self.version
    }

    pub fn entries(&self) -> &[(StatsColumnTarget, HistogramPlaceholder)] {
        &self.entries
    }

    pub fn digest(&self) -> StatsPublicationDigest {
        self.digest
    }

    pub fn summary(&self) -> StatsPublicationSummary {
        StatsPublicationSummary::from_publication(self)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn validate(&self) -> Result<(), StatsValidationError> {
        if self.catalog_version.get() == 0 {
            return Err(StatsValidationError::CatalogVersionZero);
        }
        if self.version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        if self.entries.len() > MAX_HISTOGRAMS_PER_PUBLICATION {
            return Err(StatsValidationError::PublicationExceedsHistogramCap);
        }

        let mut previous_key: Option<(u64, u16)> = None;
        for (target, histogram) in &self.entries {
            if target.object_id.get() == 0 {
                return Err(StatsValidationError::ZeroObjectId);
            }
            let key = target.canonical_key();
            if let Some(previous) = previous_key {
                if previous == key {
                    return Err(StatsValidationError::DuplicateTarget);
                }
                if previous > key {
                    return Err(StatsValidationError::PublicationTargetsNotCanonical);
                }
            }
            validate_histogram(histogram)?;
            previous_key = Some(key);
        }

        let expected =
            compute_publication_digest(self.catalog_version, self.version, &self.entries);
        if expected != self.digest {
            return Err(StatsValidationError::PublicationDigestMismatch);
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StatsPublicationBuilder {
    catalog_version: CatalogVersion,
    version: StatsVersion,
    entries: Vec<(StatsColumnTarget, HistogramPlaceholder)>,
}

impl StatsPublicationBuilder {
    pub fn new(
        catalog_version: CatalogVersion,
        version: StatsVersion,
    ) -> Result<Self, StatsValidationError> {
        if catalog_version.get() == 0 {
            return Err(StatsValidationError::CatalogVersionZero);
        }
        if version.get() == 0 {
            return Err(StatsValidationError::StatsVersionZero);
        }
        Ok(Self {
            catalog_version,
            version,
            entries: Vec::new(),
        })
    }

    pub fn push(
        mut self,
        target: StatsColumnTarget,
        histogram: HistogramPlaceholder,
    ) -> Result<Self, StatsValidationError> {
        if target.object_id.get() == 0 {
            return Err(StatsValidationError::ZeroObjectId);
        }
        if self.entries.len() >= MAX_HISTOGRAMS_PER_PUBLICATION {
            return Err(StatsValidationError::PublicationExceedsHistogramCap);
        }
        if self.entries.iter().any(|(existing, _)| *existing == target) {
            return Err(StatsValidationError::DuplicateTarget);
        }
        self.entries.push((target, histogram));
        Ok(self)
    }

    pub fn finish(mut self) -> StatsPublication {
        self.entries
            .sort_by_key(|(target, _)| target.canonical_key());
        let digest = compute_publication_digest(self.catalog_version, self.version, &self.entries);
        StatsPublication {
            catalog_version: self.catalog_version,
            version: self.version,
            entries: self.entries,
            digest,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPublicationDecisionStage {
    Candidate,
    Validate,
    Publish,
    Reject,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPublicationSwitchDecision {
    KeepActive,
    PublishCandidate,
    RestorePreviousPublication,
}

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

    pub fn from_advisory_use(
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

    pub fn matches_advisory_use(self, advisory_use: ScenarioEvidenceAdvisoryUse) -> bool {
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

    pub fn new_with_advisory_reference(
        kind: StatsPublicationDecisionEvidenceKind,
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        let advisory_reference =
            StatsPublicationAdvisoryEvidenceReference::from_advisory_use(advisory_use)?;
        Self::new_with_advisory_identity(kind, trace_id, advisory_use, advisory_reference)
    }

    pub fn new_with_advisory_identity(
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

    pub fn operator_review_with_advisory_reference(
        trace_id: TraceId,
        advisory_use: ScenarioEvidenceAdvisoryUse,
    ) -> Result<Self, StatsPublicationSwitchError> {
        Self::new_with_advisory_reference(
            StatsPublicationDecisionEvidenceKind::OperatorReview,
            trace_id,
            advisory_use,
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
        !self.is_predictive_only()
    }

    pub const fn advisory_evidence_can_drive_active_stats_version_transition(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsPublicationDecisionTrace {
    pub trace_id: TraceId,
    pub stage: StatsPublicationDecisionStage,
    pub selected_decision: StatsPublicationSwitchDecision,
    pub decision_evidence: StatsPublicationDecisionEvidence,
    pub active_before: Option<StatsPublicationSummary>,
    pub candidate: Option<StatsPublicationSummary>,
    pub active_after: Option<StatsPublicationSummary>,
    pub accepted: bool,
    pub reason: String,
}

impl StatsPublicationDecisionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub fn active_changed(&self) -> bool {
        self.active_before != self.active_after
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPublicationCandidateState {
    Staged,
    Validated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingStatsPublicationCandidate {
    publication: StatsPublication,
    state: StatsPublicationCandidateState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsPublicationSwitchError {
    TraceIdZero,
    DecisionEvidenceTraceIdZero,
    EmptyReason,
    ReasonTooLong,
    PredictiveEvidenceCannotDriveActiveStatsVersion,
    PredictiveEvidenceRequiresAdvisoryIdentity,
    AdvisoryEvidenceCannotDriveActiveStatsVersion,
    AdvisoryEvidenceDigestZero,
    AdvisoryEvidenceTargetRejected(ScenarioEvidenceError),
    AdvisoryEvidenceIdentityMismatch,
    AdvisoryEvidenceStatsVersionMismatch {
        expected: StatsVersion,
        actual: StatsVersion,
    },
    PendingCandidateExists,
    NoPendingCandidate,
    PendingCandidateNotValidated,
    CandidateVersionNotAdvanced {
        active: StatsVersion,
        candidate: StatsVersion,
    },
    NoPreviousPublication,
    ValidationRejected(StatsValidationError),
}

impl core::fmt::Display for StatsPublicationSwitchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StatsPublicationSwitchError::TraceIdZero => {
                f.write_str("stats publication switch trace_id must be non-zero")
            }
            StatsPublicationSwitchError::DecisionEvidenceTraceIdZero => {
                f.write_str("stats publication decision evidence trace_id must be non-zero")
            }
            StatsPublicationSwitchError::EmptyReason => {
                f.write_str("stats publication switch reason must not be empty")
            }
            StatsPublicationSwitchError::ReasonTooLong => {
                f.write_str("stats publication switch reason exceeds the bounded byte limit")
            }
            StatsPublicationSwitchError::PredictiveEvidenceCannotDriveActiveStatsVersion => f
                .write_str(
                    "predictive ScenarioEvidence cannot drive an active StatsVersion transition",
                ),
            StatsPublicationSwitchError::PredictiveEvidenceRequiresAdvisoryIdentity => f.write_str(
                "predictive ScenarioEvidence decision evidence requires explicit advisory identity",
            ),
            StatsPublicationSwitchError::AdvisoryEvidenceCannotDriveActiveStatsVersion => f
                .write_str(
                    "advisory ScenarioEvidence cannot drive an active StatsVersion transition",
                ),
            StatsPublicationSwitchError::AdvisoryEvidenceDigestZero => {
                f.write_str("stats publication advisory evidence digest must be non-zero")
            }
            StatsPublicationSwitchError::AdvisoryEvidenceTargetRejected(err) => {
                write!(f, "stats publication advisory evidence target rejected: {err}")
            }
            StatsPublicationSwitchError::AdvisoryEvidenceIdentityMismatch => f.write_str(
                "stats publication advisory evidence identity does not match the validated advisory token",
            ),
            StatsPublicationSwitchError::AdvisoryEvidenceStatsVersionMismatch {
                expected,
                actual,
            } => write!(
                f,
                "stats publication advisory evidence targets StatsVersion {}, expected {}",
                actual.get(),
                expected.get()
            ),
            StatsPublicationSwitchError::PendingCandidateExists => {
                f.write_str("stats publication switch already has a pending candidate")
            }
            StatsPublicationSwitchError::NoPendingCandidate => {
                f.write_str("stats publication switch has no pending candidate")
            }
            StatsPublicationSwitchError::PendingCandidateNotValidated => {
                f.write_str("stats publication candidate must be validated before publish")
            }
            StatsPublicationSwitchError::CandidateVersionNotAdvanced { active, candidate } => {
                write!(
                    f,
                    "candidate StatsVersion {} must advance active StatsVersion {}",
                    candidate.get(),
                    active.get()
                )
            }
            StatsPublicationSwitchError::NoPreviousPublication => {
                f.write_str("stats publication switch has no previous active publication")
            }
            StatsPublicationSwitchError::ValidationRejected(err) => {
                write!(f, "stats publication candidate validation rejected: {err}")
            }
        }
    }
}

impl std::error::Error for StatsPublicationSwitchError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatsPublicationSwitch {
    active: Option<StatsPublication>,
    previous_active: Option<StatsPublication>,
    pending: Option<PendingStatsPublicationCandidate>,
    history: Vec<StatsPublicationDecisionTrace>,
}

impl StatsPublicationSwitch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> Option<&StatsPublication> {
        self.active.as_ref()
    }

    pub fn active_summary(&self) -> Option<StatsPublicationSummary> {
        self.active.as_ref().map(StatsPublication::summary)
    }

    pub fn pending_candidate(&self) -> Option<&StatsPublication> {
        self.pending
            .as_ref()
            .map(|candidate| &candidate.publication)
    }

    pub fn pending_state(&self) -> Option<StatsPublicationCandidateState> {
        self.pending.as_ref().map(|candidate| candidate.state)
    }

    pub fn history(&self) -> &[StatsPublicationDecisionTrace] {
        &self.history
    }

    pub fn stage_candidate(
        &mut self,
        publication: StatsPublication,
        trace_id: TraceId,
        reason: impl Into<String>,
    ) -> Result<StatsPublicationDecisionTrace, StatsPublicationSwitchError> {
        validate_switch_trace_id(trace_id)?;
        let decision_evidence = StatsPublicationDecisionEvidence::stats_builder(trace_id)?;
        if self.pending.is_some() {
            return Err(StatsPublicationSwitchError::PendingCandidateExists);
        }

        let active = self.active_summary();
        let candidate = publication.summary();
        let trace = publication_trace(
            trace_id,
            StatsPublicationDecisionStage::Candidate,
            StatsPublicationSwitchDecision::KeepActive,
            decision_evidence,
            active,
            Some(candidate),
            active,
            true,
            reason,
        )?;

        self.pending = Some(PendingStatsPublicationCandidate {
            publication,
            state: StatsPublicationCandidateState::Staged,
        });
        self.push_history(trace.clone());
        Ok(trace)
    }

    pub fn validate_candidate(
        &mut self,
        trace_id: TraceId,
        reason: impl Into<String>,
    ) -> Result<StatsPublicationDecisionTrace, StatsPublicationSwitchError> {
        validate_switch_trace_id(trace_id)?;
        let decision_evidence = StatsPublicationDecisionEvidence::canonical_validation(trace_id)?;
        let reason = normalize_reason(reason)?;
        let Some(pending) = self.pending.as_ref() else {
            return Err(StatsPublicationSwitchError::NoPendingCandidate);
        };

        let active = self.active_summary();
        let candidate = pending.publication.summary();
        let validation_outcome = self.validate_pending_candidate(&pending.publication);
        let (accepted, reason) = match validation_outcome {
            Ok(()) => (true, reason),
            Err(err) => (false, format!("{reason}: {err}")),
        };

        if accepted {
            if let Some(pending) = self.pending.as_mut() {
                pending.state = StatsPublicationCandidateState::Validated;
            }
        } else {
            self.pending = None;
        }

        let trace = publication_trace(
            trace_id,
            StatsPublicationDecisionStage::Validate,
            StatsPublicationSwitchDecision::KeepActive,
            decision_evidence,
            active,
            Some(candidate),
            active,
            accepted,
            reason,
        )?;
        self.push_history(trace.clone());
        Ok(trace)
    }

    pub fn publish_validated_candidate(
        &mut self,
        trace_id: TraceId,
        decision_evidence: StatsPublicationDecisionEvidence,
        reason: impl Into<String>,
    ) -> Result<StatsPublicationDecisionTrace, StatsPublicationSwitchError> {
        validate_switch_trace_id(trace_id)?;
        let reason = normalize_reason(reason)?;
        let Some(pending) = self.pending.as_ref() else {
            return Err(StatsPublicationSwitchError::NoPendingCandidate);
        };
        if pending.state != StatsPublicationCandidateState::Validated {
            return Err(StatsPublicationSwitchError::PendingCandidateNotValidated);
        }
        validate_active_transition_evidence(decision_evidence, pending.publication.version)?;

        let Some(pending) = self.pending.take() else {
            return Err(StatsPublicationSwitchError::NoPendingCandidate);
        };

        self.validate_pending_candidate(&pending.publication)?;

        let active_before = self.active_summary();
        let candidate = pending.publication.summary();
        self.previous_active = self.active.clone();
        self.active = Some(pending.publication);
        let active_after = self.active_summary();

        let trace = publication_trace(
            trace_id,
            StatsPublicationDecisionStage::Publish,
            StatsPublicationSwitchDecision::PublishCandidate,
            decision_evidence,
            active_before,
            Some(candidate),
            active_after,
            true,
            reason,
        )?;
        self.push_history(trace.clone());
        Ok(trace)
    }

    pub fn reject_candidate(
        &mut self,
        trace_id: TraceId,
        reason: impl Into<String>,
    ) -> Result<StatsPublicationDecisionTrace, StatsPublicationSwitchError> {
        validate_switch_trace_id(trace_id)?;
        let decision_evidence = StatsPublicationDecisionEvidence::operator_review(trace_id)?;
        let reason = normalize_reason(reason)?;
        let Some(pending) = self.pending.take() else {
            return Err(StatsPublicationSwitchError::NoPendingCandidate);
        };

        let active = self.active_summary();
        let trace = publication_trace(
            trace_id,
            StatsPublicationDecisionStage::Reject,
            StatsPublicationSwitchDecision::KeepActive,
            decision_evidence,
            active,
            Some(pending.publication.summary()),
            active,
            false,
            reason,
        )?;
        self.push_history(trace.clone());
        Ok(trace)
    }

    pub fn rollback_last_publish(
        &mut self,
        trace_id: TraceId,
        decision_evidence: StatsPublicationDecisionEvidence,
        reason: impl Into<String>,
    ) -> Result<StatsPublicationDecisionTrace, StatsPublicationSwitchError> {
        validate_switch_trace_id(trace_id)?;
        let reason = normalize_reason(reason)?;
        let Some(previous) = self.previous_active.as_ref() else {
            return Err(StatsPublicationSwitchError::NoPreviousPublication);
        };
        let active_before = self.active_summary();
        let expected_version = active_before
            .map(|summary| summary.version)
            .unwrap_or_else(|| previous.version());
        validate_active_transition_evidence(decision_evidence, expected_version)?;

        let Some(previous) = self.previous_active.take() else {
            return Err(StatsPublicationSwitchError::NoPreviousPublication);
        };

        let current = self.active.replace(previous);
        self.previous_active = current;
        let active_after = self.active_summary();

        let trace = publication_trace(
            trace_id,
            StatsPublicationDecisionStage::Rollback,
            StatsPublicationSwitchDecision::RestorePreviousPublication,
            decision_evidence,
            active_before,
            None,
            active_after,
            true,
            reason,
        )?;
        self.push_history(trace.clone());
        Ok(trace)
    }

    fn validate_pending_candidate(
        &self,
        publication: &StatsPublication,
    ) -> Result<(), StatsPublicationSwitchError> {
        publication
            .validate()
            .map_err(StatsPublicationSwitchError::ValidationRejected)?;
        if let Some(active) = &self.active
            && publication.version <= active.version
        {
            return Err(StatsPublicationSwitchError::CandidateVersionNotAdvanced {
                active: active.version,
                candidate: publication.version,
            });
        }
        Ok(())
    }

    fn push_history(&mut self, trace: StatsPublicationDecisionTrace) {
        self.history.push(trace);
        let overflow = self
            .history
            .len()
            .saturating_sub(STATS_PUBLICATION_SWITCH_HISTORY_LIMIT);
        if overflow > 0 {
            self.history.drain(..overflow);
        }
    }
}

fn validate_histogram(histogram: &HistogramPlaceholder) -> Result<(), StatsValidationError> {
    if histogram.buckets().is_empty() {
        return Err(StatsValidationError::HistogramHasNoBuckets);
    }
    if histogram.buckets().len() > MAX_BUCKETS_PER_HISTOGRAM {
        return Err(StatsValidationError::HistogramExceedsBucketCap);
    }

    let mut previous_upper = None;
    for bucket in histogram.buckets() {
        bucket.validate()?;
        if let Some(previous) = previous_upper
            && bucket.lower_inclusive <= previous
        {
            return Err(StatsValidationError::BucketsNotMonotonic);
        }
        previous_upper = Some(bucket.upper_inclusive);
    }
    Ok(())
}

fn compute_publication_digest(
    catalog_version: CatalogVersion,
    version: StatsVersion,
    entries: &[(StatsColumnTarget, HistogramPlaceholder)],
) -> StatsPublicationDigest {
    let mut hasher = Sha256::new();
    hasher.update(STATS_PUBLICATION_DOMAIN);
    hasher.update(&[0xD0]);
    hasher.update(&catalog_version.get().to_le_bytes());
    hasher.update(&[0xD1]);
    hasher.update(&version.get().to_le_bytes());
    hasher.update(&[0xD2]);
    hasher.update(&(entries.len() as u32).to_le_bytes());
    for (target, histogram) in entries {
        hasher.update(&[0xD3]);
        hasher.update(&target.object_id.get().to_le_bytes());
        hasher.update(&target.column_index.to_le_bytes());
        histogram.absorb(&mut hasher);
    }

    StatsPublicationDigest::from_bytes(hasher.finalize())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Stats publication trace construction keeps every audited switch field explicit."
)]
fn publication_trace(
    trace_id: TraceId,
    stage: StatsPublicationDecisionStage,
    selected_decision: StatsPublicationSwitchDecision,
    decision_evidence: StatsPublicationDecisionEvidence,
    active_before: Option<StatsPublicationSummary>,
    candidate: Option<StatsPublicationSummary>,
    active_after: Option<StatsPublicationSummary>,
    accepted: bool,
    reason: impl Into<String>,
) -> Result<StatsPublicationDecisionTrace, StatsPublicationSwitchError> {
    if trace_id.is_zero() {
        return Err(StatsPublicationSwitchError::TraceIdZero);
    }
    let reason = publication_reason(
        stage,
        selected_decision,
        decision_evidence,
        active_before,
        candidate,
        active_after,
        accepted,
        reason,
    )?;
    Ok(StatsPublicationDecisionTrace {
        trace_id,
        stage,
        selected_decision,
        decision_evidence,
        active_before,
        candidate,
        active_after,
        accepted,
        reason,
    })
}

fn normalize_reason(reason: impl Into<String>) -> Result<String, StatsPublicationSwitchError> {
    let reason = reason.into();
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(StatsPublicationSwitchError::EmptyReason);
    }
    if reason.len() > STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES {
        return Err(StatsPublicationSwitchError::ReasonTooLong);
    }
    Ok(reason.to_string())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Stats publication reason strings include every audited switch field explicitly."
)]
fn publication_reason(
    stage: StatsPublicationDecisionStage,
    selected_decision: StatsPublicationSwitchDecision,
    decision_evidence: StatsPublicationDecisionEvidence,
    active_before: Option<StatsPublicationSummary>,
    candidate: Option<StatsPublicationSummary>,
    active_after: Option<StatsPublicationSummary>,
    accepted: bool,
    reason: impl Into<String>,
) -> Result<String, StatsPublicationSwitchError> {
    let reason = normalize_reason(reason)?;
    Ok(format!(
        "{} stage={:?} selected_decision={:?} decision_evidence={} accepted={} active_before={} candidate={} active_after={}",
        reason,
        stage,
        selected_decision,
        decision_evidence_reason(decision_evidence),
        accepted,
        summary_reason(active_before),
        summary_reason(candidate),
        summary_reason(active_after)
    ))
}

fn validate_switch_trace_id(trace_id: TraceId) -> Result<(), StatsPublicationSwitchError> {
    if trace_id.is_zero() {
        Err(StatsPublicationSwitchError::TraceIdZero)
    } else {
        Ok(())
    }
}

fn validate_active_transition_evidence(
    decision_evidence: StatsPublicationDecisionEvidence,
    expected_stats_version: StatsVersion,
) -> Result<(), StatsPublicationSwitchError> {
    if !decision_evidence.can_drive_active_stats_version_transition() {
        return Err(StatsPublicationSwitchError::PredictiveEvidenceCannotDriveActiveStatsVersion);
    }
    if let Some(advisory) = decision_evidence.advisory_evidence_reference() {
        let actual = advisory.target().stats_version;
        if actual != expected_stats_version {
            return Err(
                StatsPublicationSwitchError::AdvisoryEvidenceStatsVersionMismatch {
                    expected: expected_stats_version,
                    actual,
                },
            );
        }
    }
    Ok(())
}

fn decision_evidence_reason(evidence: StatsPublicationDecisionEvidence) -> String {
    format!(
        "kind:{:?},trace:{},can_drive_active:{},advisory_can_drive_active:{},advisory:{}",
        evidence.kind(),
        evidence.trace_id().get(),
        evidence.can_drive_active_stats_version_transition(),
        evidence.advisory_evidence_can_drive_active_stats_version_transition(),
        advisory_evidence_reason(evidence.advisory_evidence_reference())
    )
}

fn advisory_evidence_reason(advisory: Option<StatsPublicationAdvisoryEvidenceReference>) -> String {
    match advisory {
        Some(advisory) => {
            let target = advisory.target();
            format!(
                "scenario:{},digest:{},target:procedure:{},catalog:{},stats:{},plan:{:?},contract:{}",
                advisory.scenario_id().get(),
                digest_prefix_hex(&advisory.digest()),
                target.procedure_id.get(),
                target.catalog_version.get(),
                target.stats_version.get(),
                target.plan_class,
                target
                    .contract_hash
                    .map(|hash| digest_prefix_hex(&hash.as_bytes()))
                    .unwrap_or_else(|| "none".to_string())
            )
        }
        None => "none".to_string(),
    }
}

fn summary_reason(summary: Option<StatsPublicationSummary>) -> String {
    match summary {
        Some(summary) => format!(
            "catalog_version:{},version:{},digest:{},entries:{}",
            summary.catalog_version.get(),
            summary.version.get(),
            digest_prefix_hex(&summary.digest.as_bytes()),
            summary.entry_count
        ),
        None => "none".to_string(),
    }
}
