use andromeda_decision_trace::digest_prefix_hex;
use andromeda_observability::TraceId;

use crate::{
    StatsPublicationAdvisoryEvidenceReference, StatsPublicationDecisionEvidence,
    StatsPublicationSummary, StatsPublicationSwitchError,
};

/// Maximum UTF-8 byte length of the operator-supplied switch reason.
pub const STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES: usize = 512;

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

#[allow(
    clippy::too_many_arguments,
    reason = "Stats publication trace construction keeps every audited switch field explicit."
)]
pub(crate) fn publication_trace(
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

pub(crate) fn normalize_reason(
    reason: impl Into<String>,
) -> Result<String, StatsPublicationSwitchError> {
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

pub(crate) fn validate_switch_trace_id(
    trace_id: TraceId,
) -> Result<(), StatsPublicationSwitchError> {
    if trace_id.is_zero() {
        Err(StatsPublicationSwitchError::TraceIdZero)
    } else {
        Ok(())
    }
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
        },
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
