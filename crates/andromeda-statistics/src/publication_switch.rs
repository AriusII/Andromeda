use andromeda_observability::TraceId;
use andromeda_procedure_contract::StatsVersion;

use crate::{
    StatsPublication, StatsPublicationDecisionEvidence, StatsPublicationDecisionStage,
    StatsPublicationDecisionTrace, StatsPublicationSummary, StatsPublicationSwitchDecision,
    StatsPublicationSwitchError,
    publication_trace::{normalize_reason, publication_trace, validate_switch_trace_id},
};

/// Maximum number of switch traces retained by a publication switch.
pub const STATS_PUBLICATION_SWITCH_HISTORY_LIMIT: usize = 128;

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

    fn active_summary(&self) -> Option<StatsPublicationSummary> {
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
        validate_active_transition_evidence(decision_evidence, pending.publication.version())?;

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
            && publication.version() <= active.version()
        {
            return Err(StatsPublicationSwitchError::CandidateVersionNotAdvanced {
                active: active.version(),
                candidate: publication.version(),
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
