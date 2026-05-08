use super::super::planning::StartupMode;
use super::{
    ObservedBoundary, StartupAcceptance, StartupDecision, StartupEvidence, StartupOutcome,
    StartupRejectionReason,
};

/// Take a startup decision under the requested mode and durable evidence.
///
/// The function is total: every input produces either
/// [`StartupOutcome::Accepted`] with a [`StartupAcceptance`] proof or
/// [`StartupOutcome::Rejected`] with a [`StartupRejectionReason`].
/// It performs no I/O and never mutates the inputs.
pub fn decide_startup(mode: StartupMode, evidence: StartupEvidence) -> StartupDecision {
    let outcome = classify(mode, &evidence);
    StartupDecision {
        mode,
        evidence,
        outcome,
    }
}

fn classify(mode: StartupMode, evidence: &StartupEvidence) -> StartupOutcome {
    if let Some(rejection) = classify_global_rejection(evidence) {
        return StartupOutcome::Rejected(rejection);
    }

    let boundary = evidence.observed_boundary();

    match mode {
        StartupMode::FastStart => classify_fast_start(boundary, evidence.forensic_report_attached),
        StartupMode::SafeStart => classify_safe_start(boundary, evidence.forensic_report_attached),
        StartupMode::ForensicStart => classify_forensic_start(boundary, evidence),
    }
}

fn classify_global_rejection(evidence: &StartupEvidence) -> Option<StartupRejectionReason> {
    if !evidence.manifest_validated {
        return Some(StartupRejectionReason::ManifestNotValidated);
    }
    if evidence.mounted_snapshot_id == 0 {
        return Some(StartupRejectionReason::MissingColdSnapshot);
    }
    if !evidence.durable_wal_covers_anchor() {
        return Some(StartupRejectionReason::RamOnlyEvidence);
    }

    None
}

fn classify_fast_start(
    boundary: ObservedBoundary,
    forensic_report_attached: bool,
) -> StartupOutcome {
    match boundary {
        ObservedBoundary::Clean => accepted_replay(boundary, forensic_report_attached),
        ObservedBoundary::RecoverableTail => {
            StartupOutcome::Rejected(StartupRejectionReason::FastStartRequiresCleanScan)
        },
        ObservedBoundary::ForensicChainBreak => {
            StartupOutcome::Rejected(StartupRejectionReason::ForensicHandlingRequired)
        },
    }
}

fn classify_safe_start(
    boundary: ObservedBoundary,
    forensic_report_attached: bool,
) -> StartupOutcome {
    match boundary {
        ObservedBoundary::Clean | ObservedBoundary::RecoverableTail => {
            accepted_replay(boundary, forensic_report_attached)
        },
        ObservedBoundary::ForensicChainBreak => {
            StartupOutcome::Rejected(StartupRejectionReason::ForensicHandlingRequired)
        },
    }
}

fn classify_forensic_start(
    boundary: ObservedBoundary,
    evidence: &StartupEvidence,
) -> StartupOutcome {
    if !evidence.forensic_report_attached {
        return StartupOutcome::Rejected(StartupRejectionReason::ForensicStartRequiresReport);
    }

    StartupOutcome::Accepted(StartupAcceptance {
        replay_allowed: false,
        observed_boundary: boundary,
        forensic_report_preserved: true,
    })
}

fn accepted_replay(
    observed_boundary: ObservedBoundary,
    forensic_report_preserved: bool,
) -> StartupOutcome {
    StartupOutcome::Accepted(StartupAcceptance {
        replay_allowed: true,
        observed_boundary,
        forensic_report_preserved,
    })
}
