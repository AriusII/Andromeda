use andromeda_wal::{Lsn, WalScanResult, WalScanStop, WalScanStopReason};

use crate::{RecoveryManifestView, StartupMode};

/// Inputs collected from durable sources before a startup decision is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupEvidence {
    /// `true` iff the manifest has been validated against cold-snapshot invariants.
    pub manifest_validated: bool,
    /// `true` iff the durable segment index bytes were decoded and verified before
    /// the WAL scan. Set to `true` in bootstrap mode when no segment index exists yet.
    pub segment_index_validated: bool,
    /// Identifier of the cold snapshot the recovery executor will mount.
    pub mounted_snapshot_id: u64,
    /// The LSN at which the manifest requires WAL replay to start.
    pub required_wal_start_lsn: Lsn,
    /// Highest LSN that survived the durable WAL scan.
    pub last_durable_lsn: Lsn,
    /// The optional stop record returned by the durable WAL scan.
    pub wal_scan_stop: Option<WalScanStop>,
    /// `true` iff a forensic report has been produced and persisted.
    pub forensic_report_attached: bool,
}

impl StartupEvidence {
    /// Build startup evidence from durable manifest + WAL scan inputs.
    pub fn from_manifest_and_wal_scan(
        manifest: &impl RecoveryManifestView,
        scan: &WalScanResult,
        forensic_report_attached: bool,
    ) -> Self {
        Self {
            manifest_validated: manifest.validate_recovery_manifest().is_ok(),
            segment_index_validated: true,
            mounted_snapshot_id: manifest.mounted_snapshot_id(),
            required_wal_start_lsn: manifest.required_wal_start_lsn(),
            last_durable_lsn: scan.last_valid_lsn.unwrap_or(Lsn::ZERO),
            wal_scan_stop: scan.stopped,
            forensic_report_attached,
        }
    }

    /// Classify the durable WAL boundary observed by the scan.
    pub const fn observed_boundary(&self) -> ObservedBoundary {
        match self.wal_scan_stop {
            None => ObservedBoundary::Clean,
            Some(stop) => match stop.reason {
                WalScanStopReason::LsnGap
                | WalScanStopReason::DuplicateOrReorderedLsn
                | WalScanStopReason::PreviousLsnMismatch => ObservedBoundary::ForensicChainBreak,
                _ => ObservedBoundary::RecoverableTail,
            },
        }
    }

    /// Returns `true` iff the durable WAL prefix covers the manifest start LSN.
    pub const fn durable_wal_covers_anchor(&self) -> bool {
        self.last_durable_lsn.get() >= self.required_wal_start_lsn.get()
            && self.required_wal_start_lsn.get() != 0
    }
}

/// Classification of the durable-WAL tail boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedBoundary {
    /// The WAL scan reached the end of durable bytes with no anomaly.
    Clean,
    /// The WAL scan stopped on a recoverable suffix issue.
    RecoverableTail,
    /// The WAL scan observed a chain break inside the durable prefix.
    ForensicChainBreak,
}

impl ObservedBoundary {
    pub const fn requires_forensic_handling(self) -> bool {
        matches!(self, Self::ForensicChainBreak)
    }
}

/// Reasons a startup attempt may be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupRejectionReason {
    ManifestNotValidated,
    SegmentIndexNotValidated,
    MissingColdSnapshot,
    RamOnlyEvidence,
    FastStartRequiresCleanScan,
    ForensicHandlingRequired,
    ForensicStartRequiresReport,
}

impl StartupRejectionReason {
    pub const fn as_static_str(self) -> &'static str {
        match self {
            Self::ManifestNotValidated => "manifest_not_validated",
            Self::SegmentIndexNotValidated => "segment_index_not_validated",
            Self::MissingColdSnapshot => "missing_cold_snapshot",
            Self::RamOnlyEvidence => "ram_only_evidence",
            Self::FastStartRequiresCleanScan => "fast_start_requires_clean_scan",
            Self::ForensicHandlingRequired => "forensic_handling_required",
            Self::ForensicStartRequiresReport => "forensic_start_requires_report",
        }
    }
}

/// Proof carried by an accepted startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupAcceptance {
    pub replay_allowed: bool,
    pub observed_boundary: ObservedBoundary,
    pub forensic_report_preserved: bool,
}

/// Outcome of a startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupOutcome {
    Accepted(StartupAcceptance),
    Rejected(StartupRejectionReason),
}

impl StartupOutcome {
    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

/// A fully-formed, auditable startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupDecision {
    pub mode: StartupMode,
    pub evidence: StartupEvidence,
    pub outcome: StartupOutcome,
}

impl StartupDecision {
    pub const fn is_accepted(&self) -> bool {
        self.outcome.is_accepted()
    }

    pub const fn rejection(&self) -> Option<StartupRejectionReason> {
        match self.outcome {
            StartupOutcome::Rejected(reason) => Some(reason),
            StartupOutcome::Accepted(_) => None,
        }
    }

    pub const fn acceptance(&self) -> Option<StartupAcceptance> {
        match self.outcome {
            StartupOutcome::Accepted(proof) => Some(proof),
            StartupOutcome::Rejected(_) => None,
        }
    }

    /// Project the decision into an audit-friendly struct that carries the
    /// mode, durable evidence, and outcome.
    pub const fn audit_projection<TraceId>(
        &self,
        trace_id: TraceId,
    ) -> StartupAuditProjection<TraceId>
    where
        TraceId: Copy,
    {
        StartupAuditProjection {
            trace_id,
            mode: self.mode,
            mounted_snapshot_id: self.evidence.mounted_snapshot_id,
            required_wal_start_lsn: self.evidence.required_wal_start_lsn,
            last_durable_lsn: self.evidence.last_durable_lsn,
            observed_boundary: self.evidence.observed_boundary(),
            forensic_report_attached: self.evidence.forensic_report_attached,
            outcome: self.outcome,
        }
    }
}

/// Auditable projection of a startup decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupAuditProjection<TraceId = u128> {
    pub trace_id: TraceId,
    pub mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub required_wal_start_lsn: Lsn,
    pub last_durable_lsn: Lsn,
    pub observed_boundary: ObservedBoundary,
    pub forensic_report_attached: bool,
    pub outcome: StartupOutcome,
}

/// Take a startup decision under the requested mode and durable evidence.
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
    if !evidence.segment_index_validated {
        return Some(StartupRejectionReason::SegmentIndexNotValidated);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline_evidence() -> StartupEvidence {
        StartupEvidence {
            manifest_validated: true,
            segment_index_validated: true,
            mounted_snapshot_id: 42,
            required_wal_start_lsn: Lsn::new(10),
            last_durable_lsn: Lsn::new(20),
            wal_scan_stop: None,
            forensic_report_attached: false,
        }
    }

    fn recoverable_tail() -> WalScanStop {
        WalScanStop {
            reason: WalScanStopReason::TruncatedRecord,
            offset: 0,
        }
    }

    fn forensic_chain_break() -> WalScanStop {
        WalScanStop {
            reason: WalScanStopReason::LsnGap,
            offset: 0,
        }
    }

    #[test]
    fn fast_start_accepts_clean_durable_wal_only() {
        let decision = decide_startup(StartupMode::FastStart, baseline_evidence());
        assert!(decision.is_accepted());
        let proof = decision.acceptance().unwrap();
        assert!(proof.replay_allowed);
        assert_eq!(proof.observed_boundary, ObservedBoundary::Clean);
    }

    #[test]
    fn fast_start_rejects_recoverable_tail_audit_skipping() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(recoverable_tail());
        let decision = decide_startup(StartupMode::FastStart, evidence);
        assert_eq!(
            decision.rejection(),
            Some(StartupRejectionReason::FastStartRequiresCleanScan)
        );
    }

    #[test]
    fn fast_start_rejects_forensic_chain_break() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(forensic_chain_break());
        let decision = decide_startup(StartupMode::FastStart, evidence);
        assert_eq!(
            decision.rejection(),
            Some(StartupRejectionReason::ForensicHandlingRequired)
        );
    }

    #[test]
    fn safe_start_accepts_recoverable_tail_with_replay() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(recoverable_tail());
        let decision = decide_startup(StartupMode::SafeStart, evidence);
        let proof = decision.acceptance().expect("safe start tolerates tail");
        assert!(proof.replay_allowed);
        assert_eq!(proof.observed_boundary, ObservedBoundary::RecoverableTail);
    }

    #[test]
    fn forensic_start_preserves_report_and_disables_replay() {
        let mut evidence = baseline_evidence();
        evidence.forensic_report_attached = true;
        evidence.wal_scan_stop = Some(forensic_chain_break());
        let decision = decide_startup(StartupMode::ForensicStart, evidence);
        let proof = decision.acceptance().expect("forensic mode accepts break");
        assert!(!proof.replay_allowed);
        assert!(proof.forensic_report_preserved);
        assert_eq!(
            proof.observed_boundary,
            ObservedBoundary::ForensicChainBreak
        );
    }

    #[test]
    fn every_mode_rejects_missing_cold_snapshot_and_ram_only_evidence() {
        for mode in [
            StartupMode::FastStart,
            StartupMode::SafeStart,
            StartupMode::ForensicStart,
        ] {
            let mut evidence = baseline_evidence();
            evidence.mounted_snapshot_id = 0;
            evidence.forensic_report_attached = true;
            assert_eq!(
                decide_startup(mode, evidence).rejection(),
                Some(StartupRejectionReason::MissingColdSnapshot)
            );

            let mut evidence = baseline_evidence();
            evidence.last_durable_lsn = Lsn::ZERO;
            evidence.forensic_report_attached = true;
            assert_eq!(
                decide_startup(mode, evidence).rejection(),
                Some(StartupRejectionReason::RamOnlyEvidence)
            );
        }
    }

    #[test]
    fn audit_projection_carries_mode_and_outcome_for_rejection() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(forensic_chain_break());
        let decision = decide_startup(StartupMode::FastStart, evidence);
        let projection = decision.audit_projection(7_u128);

        assert_eq!(projection.mode, StartupMode::FastStart);
        assert_eq!(projection.trace_id, 7);
        assert_eq!(
            projection.observed_boundary,
            ObservedBoundary::ForensicChainBreak
        );
        assert!(matches!(projection.outcome, StartupOutcome::Rejected(_)));
        assert_eq!(projection.last_durable_lsn, evidence.last_durable_lsn);
        assert_eq!(
            projection.required_wal_start_lsn,
            evidence.required_wal_start_lsn
        );
    }
}
