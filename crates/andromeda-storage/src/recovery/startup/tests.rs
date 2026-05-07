use super::super::planning::StartupMode;
use super::*;
use crate::{Lsn, WalScanStop, WalScanStopReason};

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
    let mut evidence = baseline_evidence();
    evidence.last_durable_lsn = Lsn::new(5);
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
    let projection = decision.audit_projection(andromeda_observe::TraceId::new(7));

    assert_eq!(projection.mode, StartupMode::FastStart);
    assert_eq!(projection.trace_id, andromeda_observe::TraceId::new(7));
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
        .observe_recovery_trace(andromeda_observe::TraceId::new(11))
        .expect("safe-start acceptance projects an observable trace");
    assert_eq!(trace.last_durable_lsn, evidence.last_durable_lsn.get());
    assert_eq!(
        trace.corruption_boundary_lsn,
        Some(evidence.last_durable_lsn.get()),
        "recoverable-tail acceptance must report the boundary LSN"
    );
    assert!(trace.proves_recovery_boundary());

    let rejected = decide_startup(StartupMode::FastStart, evidence);
    assert!(rejected.rejection().is_some());
    assert!(
        rejected
            .observe_recovery_trace(andromeda_observe::TraceId::new(12))
            .is_none()
    );
}

#[test]
fn accepted_clean_scan_has_no_corruption_boundary_in_trace() {
    let evidence = baseline_evidence();
    let decision = decide_startup(StartupMode::FastStart, evidence);
    let trace = decision
        .observe_recovery_trace(andromeda_observe::TraceId::new(13))
        .unwrap();
    assert_eq!(
        trace.corruption_boundary_lsn, None,
        "clean fast-start must not synthesize a corruption boundary"
    );
}
