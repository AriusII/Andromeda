//! Explicit startup-mode model for `FastStart`, `SafeStart`, and
//! `ForensicStart`.
//!
//! # Doctrine
//!
//! * **RAM is never truth.** Every accepted startup decision must point at a
//!   mounted cold snapshot plus a durable WAL prefix. A startup attempt that
//!   only carries hot/in-memory evidence is rejected, regardless of mode.
//! * **Reconstructible truth = cold snapshot + durable WAL.** The
//!   [`StartupEvidence`] struct captures only those two sources. Its fields
//!   are projections of the validated [`crate::DatabaseManifest`] and the
//!   durable WAL scan; nothing here may be populated from RAM-only state.
//! * **Recovery / startup decisions must be auditable.** Every
//!   [`StartupDecision`] carries the mode it was taken under, the evidence
//!   it observed, and either an [`StartupAcceptance`] proof or a
//!   [`StartupRejectionReason`]. The [`StartupAuditProjection`] returned by
//!   [`StartupDecision::audit_projection`] is the structured shape that
//!   the observer plane is expected to emit alongside
//!   [`andromeda_observe::RecoveryTrace`].
//! * **Visible commit requires durable WAL evidence.** This module never
//!   "promotes" or replays anything; it only classifies what is acceptable
//!   to start from. Replay itself is owned by [`crate::RecoveryPlan`] /
//!   [`crate::ConceptualRedoPlan`].
//! * **`ForensicStart` preserves forensic report evidence without mutating
//!   truth.** When forensic mode is selected, the decision flips
//!   `replay_allowed` to `false`: the database may be inspected, but the
//!   startup path may not apply WAL records on top of the cold snapshot.

use andromeda_observe::TraceId;

use crate::{DatabaseManifest, Lsn, WalScanResult, WalScanStop, WalScanStopReason};

use super::planning::StartupMode;

/// Inputs collected from durable sources before a startup decision is taken.
///
/// All fields must be derived from the validated [`crate::DatabaseManifest`]
/// and the durable WAL byte stream. Populating any field from RAM-only state
/// is a doctrine violation and will be classified as
/// [`StartupRejectionReason::RamOnlyEvidence`] by [`decide_startup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupEvidence {
    /// `true` iff the manifest has been validated against the cold-snapshot
    /// invariants (see [`crate::DatabaseManifest::validate`]).
    pub manifest_validated: bool,
    /// Identifier of the cold snapshot the recovery executor will mount.
    /// `0` means "no cold snapshot present" and is always rejected.
    pub mounted_snapshot_id: u64,
    /// The LSN at which the manifest requires WAL replay to start
    /// (`required_wal_start_lsn`). This anchors the durable replay range.
    pub required_wal_start_lsn: Lsn,
    /// Highest LSN that survived the durable WAL scan. Must be derived from
    /// bytes that were `fsync`'d to the WAL device. RAM-only appends MUST
    /// NOT be reflected here.
    pub last_durable_lsn: Lsn,
    /// The optional stop record returned by the durable WAL scan. `None`
    /// means a clean tail; otherwise the reason classifies the boundary.
    pub wal_scan_stop: Option<WalScanStop>,
    /// `true` iff a forensic report has been produced and persisted (e.g.
    /// [`crate::FileWalRecoveryReportV0`]). Forensic startup is rejected
    /// without it.
    pub forensic_report_attached: bool,
}

impl StartupEvidence {
    /// Build startup evidence from durable manifest + WAL scan inputs.
    ///
    /// This helper deliberately records manifest validation as evidence instead
    /// of returning an error. The caller can therefore pass the result to
    /// [`decide_startup`] and obtain an auditable rejection when the manifest
    /// is invalid, rather than losing the startup attempt behind an early I/O
    /// style error.
    pub fn from_manifest_and_wal_scan(
        manifest: &DatabaseManifest,
        scan: &WalScanResult,
        forensic_report_attached: bool,
    ) -> Self {
        Self {
            manifest_validated: manifest.validate().is_ok(),
            mounted_snapshot_id: manifest.snapshot_id,
            required_wal_start_lsn: manifest.required_wal_start_lsn,
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

    /// Returns `true` iff the durable WAL prefix actually covers the
    /// manifest's required start LSN. When `false`, the decision must be
    /// rejected: no RAM state may stand in for the missing durable bytes.
    pub const fn durable_wal_covers_anchor(&self) -> bool {
        // Both LSNs are durable-derived; an empty / RAM-only WAL surfaces as
        // `last_durable_lsn` strictly less than the manifest anchor.
        self.last_durable_lsn.get() >= self.required_wal_start_lsn.get()
            && self.required_wal_start_lsn.get() != 0
    }
}

/// Classification of the durable-WAL tail boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedBoundary {
    /// The WAL scan reached the end of durable bytes with no anomaly.
    Clean,
    /// The WAL scan stopped on a recoverable suffix issue (truncated or
    /// corrupt trailing record). Truth before that boundary is intact.
    RecoverableTail,
    /// The WAL scan observed a chain break inside the durable prefix
    /// (LSN gap, duplicate / reordered LSN, or previous-LSN mismatch).
    /// This requires forensic intervention; non-forensic modes must
    /// refuse to start.
    ForensicChainBreak,
}

impl ObservedBoundary {
    pub const fn requires_forensic_handling(self) -> bool {
        matches!(self, Self::ForensicChainBreak)
    }
}

/// Reasons a startup attempt may be rejected. Each variant maps to a
/// doctrine invariant and is intended to be projected into an audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupRejectionReason {
    /// The manifest has not been validated; cold-snapshot truth is unknown.
    ManifestNotValidated,
    /// No cold snapshot is mounted (`mounted_snapshot_id == 0`).
    MissingColdSnapshot,
    /// The durable WAL prefix does not cover the manifest's required start
    /// LSN. This includes the case where only RAM-buffered records exist.
    RamOnlyEvidence,
    /// `FastStart` was requested but the WAL scan exposed a recoverable
    /// tail boundary. FastStart cannot skip a durable-tail audit.
    FastStartRequiresCleanScan,
    /// A non-forensic mode was requested but the WAL scan exposed a chain
    /// break that requires forensic handling.
    ForensicHandlingRequired,
    /// `ForensicStart` was requested without a forensic report attached.
    ForensicStartRequiresReport,
}

impl StartupRejectionReason {
    pub const fn as_static_str(self) -> &'static str {
        match self {
            Self::ManifestNotValidated => "manifest_not_validated",
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
    /// `true` iff the recovery executor is allowed to apply WAL records on
    /// top of the mounted cold snapshot under this mode.
    ///
    /// `ForensicStart` always sets this to `false`: the engine may be
    /// inspected but startup must not mutate durable truth.
    pub replay_allowed: bool,
    /// The boundary the decision was taken against.
    pub observed_boundary: ObservedBoundary,
    /// `true` iff a forensic report should be retained alongside this
    /// decision. Forensic mode always preserves the report; non-forensic
    /// modes only do so when one was supplied.
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
    /// mode, the durable evidence, and the outcome. Intended to be emitted
    /// by the observer plane alongside [`andromeda_observe::RecoveryTrace`].
    pub const fn audit_projection(&self, trace_id: TraceId) -> StartupAuditProjection {
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

    /// Project the decision into the observer-plane recovery trace, only
    /// when the decision was accepted. Rejections must be audited via
    /// [`Self::audit_projection`] instead — they do not represent a
    /// durable recovery boundary.
    pub fn observe_recovery_trace(
        &self,
        trace_id: TraceId,
    ) -> Option<andromeda_observe::RecoveryTrace> {
        let acceptance = self.acceptance()?;
        let corruption_boundary_lsn = match acceptance.observed_boundary {
            ObservedBoundary::Clean => None,
            ObservedBoundary::RecoverableTail | ObservedBoundary::ForensicChainBreak => {
                Some(self.evidence.last_durable_lsn.get())
            }
        };
        Some(andromeda_observe::RecoveryTrace {
            trace_id,
            last_durable_lsn: self.evidence.last_durable_lsn.get(),
            corruption_boundary_lsn,
        })
    }
}

/// Auditable projection of a startup decision. Carries mode + durable
/// evidence + outcome; safe to serialize into a critical-decision event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupAuditProjection {
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
    // Doctrine-wide checks (apply to every mode, including ForensicStart).
    if !evidence.manifest_validated {
        return StartupOutcome::Rejected(StartupRejectionReason::ManifestNotValidated);
    }
    if evidence.mounted_snapshot_id == 0 {
        return StartupOutcome::Rejected(StartupRejectionReason::MissingColdSnapshot);
    }
    if !evidence.durable_wal_covers_anchor() {
        return StartupOutcome::Rejected(StartupRejectionReason::RamOnlyEvidence);
    }

    let boundary = evidence.observed_boundary();

    match mode {
        StartupMode::FastStart => {
            // FastStart is the strictest fast-path: it cannot skip
            // durable-tail or chain-break audits.
            match boundary {
                ObservedBoundary::Clean => StartupOutcome::Accepted(StartupAcceptance {
                    replay_allowed: true,
                    observed_boundary: boundary,
                    forensic_report_preserved: evidence.forensic_report_attached,
                }),
                ObservedBoundary::RecoverableTail => {
                    StartupOutcome::Rejected(StartupRejectionReason::FastStartRequiresCleanScan)
                }
                ObservedBoundary::ForensicChainBreak => {
                    StartupOutcome::Rejected(StartupRejectionReason::ForensicHandlingRequired)
                }
            }
        }
        StartupMode::SafeStart => {
            // SafeStart accepts a recoverable tail (truncated/corrupt
            // suffix), but a chain-break inside the durable prefix must
            // route to forensic handling.
            match boundary {
                ObservedBoundary::Clean | ObservedBoundary::RecoverableTail => {
                    StartupOutcome::Accepted(StartupAcceptance {
                        replay_allowed: true,
                        observed_boundary: boundary,
                        forensic_report_preserved: evidence.forensic_report_attached,
                    })
                }
                ObservedBoundary::ForensicChainBreak => {
                    StartupOutcome::Rejected(StartupRejectionReason::ForensicHandlingRequired)
                }
            }
        }
        StartupMode::ForensicStart => {
            if !evidence.forensic_report_attached {
                return StartupOutcome::Rejected(
                    StartupRejectionReason::ForensicStartRequiresReport,
                );
            }
            // Forensic startup never mutates truth: the engine mounts the
            // cold snapshot read-only and preserves the forensic report
            // for inspection. Replay is explicitly disabled.
            StartupOutcome::Accepted(StartupAcceptance {
                replay_allowed: false,
                observed_boundary: boundary,
                forensic_report_preserved: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WalScanStop;

    fn baseline_evidence() -> StartupEvidence {
        StartupEvidence {
            manifest_validated: true,
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
    fn safe_start_requires_durable_wal_anchor() {
        // Stronger evidence than FastStart in the sense that SafeStart still
        // refuses to accept RAM-only evidence (anchor not covered).
        let mut evidence = baseline_evidence();
        evidence.last_durable_lsn = Lsn::new(5); // below required_wal_start_lsn
        let decision = decide_startup(StartupMode::SafeStart, evidence);
        assert_eq!(
            decision.rejection(),
            Some(StartupRejectionReason::RamOnlyEvidence)
        );
    }

    #[test]
    fn safe_start_routes_chain_break_to_forensic() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(forensic_chain_break());
        let decision = decide_startup(StartupMode::SafeStart, evidence);
        assert_eq!(
            decision.rejection(),
            Some(StartupRejectionReason::ForensicHandlingRequired)
        );
    }

    #[test]
    fn forensic_start_requires_report() {
        let evidence = baseline_evidence();
        assert!(!evidence.forensic_report_attached);
        let decision = decide_startup(StartupMode::ForensicStart, evidence);
        assert_eq!(
            decision.rejection(),
            Some(StartupRejectionReason::ForensicStartRequiresReport)
        );
    }

    #[test]
    fn forensic_start_preserves_report_and_disables_replay() {
        let mut evidence = baseline_evidence();
        evidence.forensic_report_attached = true;
        evidence.wal_scan_stop = Some(forensic_chain_break());
        let decision = decide_startup(StartupMode::ForensicStart, evidence);
        let proof = decision.acceptance().expect("forensic mode accepts break");
        assert!(
            !proof.replay_allowed,
            "ForensicStart must never mutate truth via replay"
        );
        assert!(proof.forensic_report_preserved);
        assert_eq!(
            proof.observed_boundary,
            ObservedBoundary::ForensicChainBreak
        );
    }

    #[test]
    fn every_mode_rejects_unvalidated_manifest() {
        for mode in [
            StartupMode::FastStart,
            StartupMode::SafeStart,
            StartupMode::ForensicStart,
        ] {
            let mut evidence = baseline_evidence();
            evidence.manifest_validated = false;
            evidence.forensic_report_attached = true;
            let decision = decide_startup(mode, evidence);
            assert_eq!(
                decision.rejection(),
                Some(StartupRejectionReason::ManifestNotValidated),
                "mode {mode:?} must reject unvalidated manifest"
            );
        }
    }

    #[test]
    fn every_mode_rejects_missing_cold_snapshot() {
        for mode in [
            StartupMode::FastStart,
            StartupMode::SafeStart,
            StartupMode::ForensicStart,
        ] {
            let mut evidence = baseline_evidence();
            evidence.mounted_snapshot_id = 0;
            evidence.forensic_report_attached = true;
            let decision = decide_startup(mode, evidence);
            assert_eq!(
                decision.rejection(),
                Some(StartupRejectionReason::MissingColdSnapshot),
                "mode {mode:?} must reject missing cold snapshot"
            );
        }
    }

    #[test]
    fn every_mode_rejects_ram_only_evidence() {
        // The doctrine: RAM is never truth. Even if the caller asks for
        // ForensicStart with a report attached, an empty / RAM-only
        // durable WAL prefix must be rejected.
        for mode in [
            StartupMode::FastStart,
            StartupMode::SafeStart,
            StartupMode::ForensicStart,
        ] {
            let mut evidence = baseline_evidence();
            evidence.last_durable_lsn = Lsn::new(0);
            evidence.required_wal_start_lsn = Lsn::new(10);
            evidence.forensic_report_attached = true;
            let decision = decide_startup(mode, evidence);
            assert_eq!(
                decision.rejection(),
                Some(StartupRejectionReason::RamOnlyEvidence),
                "mode {mode:?} must reject RAM-only evidence"
            );
        }
    }

    #[test]
    fn audit_projection_carries_mode_and_outcome_for_rejection() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(forensic_chain_break());
        let decision = decide_startup(StartupMode::FastStart, evidence);
        let projection = decision.audit_projection(TraceId::new(7));

        assert_eq!(projection.mode, StartupMode::FastStart);
        assert_eq!(projection.trace_id, TraceId::new(7));
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

    #[test]
    fn observe_recovery_trace_only_emitted_for_accepted_decisions() {
        let mut evidence = baseline_evidence();
        evidence.wal_scan_stop = Some(recoverable_tail());
        let accepted = decide_startup(StartupMode::SafeStart, evidence);
        let trace = accepted
            .observe_recovery_trace(TraceId::new(11))
            .expect("safe-start acceptance projects an observable trace");
        assert_eq!(trace.last_durable_lsn, evidence.last_durable_lsn.get());
        assert_eq!(
            trace.corruption_boundary_lsn,
            Some(evidence.last_durable_lsn.get()),
            "recoverable-tail acceptance must report the boundary LSN"
        );
        assert!(trace.proves_recovery_boundary());

        // FastStart rejection: must NOT yield an observable recovery
        // boundary (rejections are audit-only).
        let rejected = decide_startup(StartupMode::FastStart, evidence);
        assert!(rejected.rejection().is_some());
        assert!(rejected.observe_recovery_trace(TraceId::new(12)).is_none());
    }

    #[test]
    fn accepted_clean_scan_has_no_corruption_boundary_in_trace() {
        let evidence = baseline_evidence();
        let decision = decide_startup(StartupMode::FastStart, evidence);
        let trace = decision.observe_recovery_trace(TraceId::new(13)).unwrap();
        assert_eq!(
            trace.corruption_boundary_lsn, None,
            "clean fast-start must not synthesize a corruption boundary"
        );
    }
}
