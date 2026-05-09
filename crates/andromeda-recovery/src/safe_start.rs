//! SafeStart recovery mode proof and invariant checks.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{Lsn, WalScanResult, WalScanStop, WalScanStopReason};

use crate::{
    ObservedBoundary, RecoveryManifestView, StartupAcceptance, StartupDecision, StartupEvidence,
    StartupMode, StartupOutcome, decide_startup,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeStartAcceptance {
    pub redo_from_lsn: Lsn,
    pub last_durable_lsn: Lsn,
    pub observed_boundary: ObservedBoundary,
    pub tail_discard: Option<SafeStartTailDiscard>,
    pub inner: StartupAcceptance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeStartTailDiscard {
    pub scan_stop: WalScanStop,
    pub discard_after_lsn: Lsn,
}

impl SafeStartAcceptance {
    pub fn was_clean_tail(&self) -> bool {
        self.tail_discard.is_none()
    }
}

pub fn safe_start_from_manifest_and_scan(
    manifest: &impl RecoveryManifestView,
    scan: &WalScanResult,
) -> AndromedaResult<SafeStartAcceptance> {
    let evidence = StartupEvidence::from_manifest_and_wal_scan(manifest, scan, false);
    let decision = decide_startup(StartupMode::SafeStart, evidence);
    safe_start_from_decision(decision)
}

pub fn safe_start_from_decision(decision: StartupDecision) -> AndromedaResult<SafeStartAcceptance> {
    match decision.outcome {
        StartupOutcome::Accepted(inner) => {
            let tail_discard =
                decision
                    .evidence
                    .wal_scan_stop
                    .map(|scan_stop| SafeStartTailDiscard {
                        scan_stop,
                        discard_after_lsn: decision.evidence.last_durable_lsn,
                    });
            Ok(SafeStartAcceptance {
                redo_from_lsn: decision.evidence.required_wal_start_lsn,
                last_durable_lsn: decision.evidence.last_durable_lsn,
                observed_boundary: inner.observed_boundary,
                tail_discard,
                inner,
            })
        },
        StartupOutcome::Rejected(reason) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "SafeStart rejected ({}): use ForensicStart or resolve the WAL chain break",
                reason.as_static_str()
            ),
        )),
    }
}

pub fn verify_safe_start_invariants(
    manifest: &impl RecoveryManifestView,
    scan: &WalScanResult,
) -> AndromedaResult<SafeStartInvariantReport> {
    let manifest_ok = manifest.validate_recovery_manifest().is_ok();
    let snapshot_ok = manifest.mounted_snapshot_id() != 0;
    let wal_covers_anchor = scan
        .last_valid_lsn
        .map(|lsn| lsn >= manifest.required_wal_start_lsn())
        .unwrap_or(false);
    let no_chain_break = scan
        .stopped
        .map(|stop| {
            !matches!(
                stop.reason,
                WalScanStopReason::LsnGap
                    | WalScanStopReason::DuplicateOrReorderedLsn
                    | WalScanStopReason::PreviousLsnMismatch
            )
        })
        .unwrap_or(true);
    let format_fingerprints_ok = manifest
        .validate_recovery_storage_formats(StartupMode::SafeStart)
        .is_ok();

    let all_pass =
        manifest_ok && snapshot_ok && wal_covers_anchor && no_chain_break && format_fingerprints_ok;

    if !all_pass {
        Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "SafeStart invariant check failed: \
                 manifest_ok={manifest_ok}, snapshot_ok={snapshot_ok}, \
                 wal_covers_anchor={wal_covers_anchor}, \
                 no_chain_break={no_chain_break}, \
                 format_fingerprints_ok={format_fingerprints_ok}"
            ),
        ))
    } else {
        Ok(SafeStartInvariantReport {
            manifest_ok,
            snapshot_ok,
            wal_covers_anchor,
            no_chain_break,
            format_fingerprints_ok,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeStartInvariantReport {
    pub manifest_ok: bool,
    pub snapshot_ok: bool,
    pub wal_covers_anchor: bool,
    pub no_chain_break: bool,
    pub format_fingerprints_ok: bool,
}

impl SafeStartInvariantReport {
    pub fn all_pass(&self) -> bool {
        self.manifest_ok
            && self.snapshot_ok
            && self.wal_covers_anchor
            && self.no_chain_break
            && self.format_fingerprints_ok
    }
}
