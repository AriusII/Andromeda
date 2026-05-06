//! SafeStart recovery mode — full recovery with all invariant checks.
//!
//! # Doctrine
//!
//! `SafeStart` is the **standard production recovery path**.  It accepts both
//! a clean WAL tail and a **recoverable tail boundary** (truncated or corrupt
//! suffix after the last valid durable record).  It rejects any WAL scan that
//! exposes a chain break inside the durable prefix (LSN gap, duplicate/reordered
//! LSN, or `previous_lsn` mismatch) — those require `ForensicStart`.
//!
//! # Full Invariant Checks (all performed)
//!
//! 1. **Manifest validation** — identity fields, CRC, WAL anchor ordering.
//! 2. **Storage format fingerprints** — all five formats at compatible versions.
//! 3. **WAL coverage** — contiguous LSN chain from `required_wal_start_lsn`.
//! 4. **Transaction consistency** — incomplete transactions identified and discarded.
//! 5. **Redo plan construction** — every durable record classified before replay.
//!
//! # Performance Budget
//!
//! SafeStart targets **< 5 seconds** for databases up to 1 GB.  For larger
//! databases the SLA is proportional to WAL replay volume.
//!
//! # Recoverable-Tail Policy
//!
//! When the WAL scan stopped at a recoverable tail (e.g. truncated last record),
//! `SafeStart` accepts the boundary and replays up to the last valid durable LSN.
//! The corrupt suffix is discarded; no data from it enters the redo set.
//!
//! # Relationship to `decide_startup`
//!
//! This module wraps [`super::startup::decide_startup`] with a SafeStart-specific
//! typed proof and optional tail-discard report.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{DatabaseManifest, Lsn, WalScanResult, WalScanStop};

use super::planning::StartupMode;
use super::startup::{
    ObservedBoundary, StartupAcceptance, StartupDecision, StartupEvidence, StartupOutcome,
    decide_startup,
};

/// Acceptance proof produced by a successful `SafeStart`.
///
/// Carries the manifest recovery floor, the durable WAL boundary, and an
/// optional description of the tail boundary that was observed and discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeStartAcceptance {
    /// The LSN from which redo must begin.
    pub redo_from_lsn: Lsn,
    /// The highest LSN that survived the durable WAL scan.
    pub last_durable_lsn: Lsn,
    /// The boundary observed by the WAL scan.
    pub observed_boundary: ObservedBoundary,
    /// Present when the scan stopped at a recoverable tail. Contains
    /// the discard evidence (the corrupt suffix is not replayed).
    pub tail_discard: Option<SafeStartTailDiscard>,
    /// Inner `StartupAcceptance` proof from `decide_startup`.
    pub inner: StartupAcceptance,
}

/// Evidence of a recoverable tail boundary discarded during SafeStart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeStartTailDiscard {
    /// The scan stop record that describes the tail anomaly.
    pub scan_stop: WalScanStop,
    /// Redo will stop at `last_durable_lsn`; bytes after this are not applied.
    pub discard_after_lsn: Lsn,
}

impl SafeStartAcceptance {
    /// Returns `true` if the WAL tail was clean (no discard needed).
    pub fn was_clean_tail(&self) -> bool {
        self.tail_discard.is_none()
    }
}

/// Attempt a `SafeStart` from manifest and WAL scan evidence.
///
/// Succeeds for clean boundaries **and** recoverable tail boundaries.
/// Rejects forensic chain breaks (LSN gap, duplicate, previous-LSN mismatch).
///
/// # Preconditions
///
/// * `manifest.validate()` must pass.
/// * `scan.last_valid_lsn` must be `>= manifest.required_wal_start_lsn`.
pub fn safe_start_from_manifest_and_scan(
    manifest: &DatabaseManifest,
    scan: &WalScanResult,
) -> AndromedaResult<SafeStartAcceptance> {
    let evidence = StartupEvidence::from_manifest_and_wal_scan(manifest, scan, false);
    let decision = decide_startup(StartupMode::SafeStart, evidence);
    safe_start_from_decision(decision, manifest)
}

/// Convert an already-taken `StartupDecision` into a `SafeStartAcceptance`.
pub fn safe_start_from_decision(
    decision: StartupDecision,
    manifest: &DatabaseManifest,
) -> AndromedaResult<SafeStartAcceptance> {
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
                redo_from_lsn: manifest.required_wal_start_lsn,
                last_durable_lsn: decision.evidence.last_durable_lsn,
                observed_boundary: inner.observed_boundary,
                tail_discard,
                inner,
            })
        }
        StartupOutcome::Rejected(reason) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "SafeStart rejected ({}): use ForensicStart or resolve the WAL chain break",
                reason.as_static_str()
            ),
        )),
    }
}

/// Verify all five SafeStart invariants against the manifest.
///
/// Returns `Ok(())` when all checks pass.  This is separate from the startup
/// decision so callers can perform the checks without building a full redo plan.
pub fn verify_safe_start_invariants(
    manifest: &DatabaseManifest,
    scan: &WalScanResult,
) -> AndromedaResult<SafeStartInvariantReport> {
    let manifest_ok = manifest.validate().is_ok();
    let snapshot_ok = manifest.snapshot_id != 0;
    let wal_covers_anchor = scan
        .last_valid_lsn
        .map(|lsn| lsn >= manifest.required_wal_start_lsn)
        .unwrap_or(false);
    let no_chain_break = scan
        .stopped
        .map(|stop| {
            !matches!(
                stop.reason,
                crate::WalScanStopReason::LsnGap
                    | crate::WalScanStopReason::DuplicateOrReorderedLsn
                    | crate::WalScanStopReason::PreviousLsnMismatch
            )
        })
        .unwrap_or(true); // no stop = clean
    let format_fingerprints_ok = manifest.storage_format_manifest().is_ok();

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

/// Report from a SafeStart invariant check.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DatabaseManifest, WalScanResult};

    fn clean_manifest() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 1,
            snapshot_id: 1,
            base_checkpoint_lsn: Lsn::ZERO,
            required_wal_start_lsn: Lsn::new(10),
            previous_manifest_hash: [0; 32],
            manifest_crc: 0xdead_beef,
        }
    }

    fn clean_scan() -> WalScanResult {
        WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(20)),
            stopped: None,
        }
    }

    #[test]
    fn safe_start_accepts_clean_boundary() {
        let result = safe_start_from_manifest_and_scan(&clean_manifest(), &clean_scan());
        assert!(result.is_ok());
        let proof = result.unwrap();
        assert!(proof.inner.replay_allowed);
        assert_eq!(proof.observed_boundary, ObservedBoundary::Clean);
        assert!(proof.was_clean_tail());
        assert!(proof.tail_discard.is_none());
    }

    #[test]
    fn safe_start_accepts_recoverable_tail() {
        use crate::{WalScanStop, WalScanStopReason};
        let manifest = clean_manifest();
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(20)),
            stopped: Some(WalScanStop {
                reason: WalScanStopReason::TruncatedRecord,
                offset: 200,
            }),
        };
        let result = safe_start_from_manifest_and_scan(&manifest, &scan);
        assert!(result.is_ok(), "SafeStart should accept recoverable tail");
        let proof = result.unwrap();
        assert!(proof.inner.replay_allowed);
        assert_eq!(proof.observed_boundary, ObservedBoundary::RecoverableTail);
        assert!(!proof.was_clean_tail());
        let discard = proof.tail_discard.unwrap();
        assert_eq!(discard.discard_after_lsn, Lsn::new(20));
    }

    #[test]
    fn safe_start_rejects_forensic_chain_break() {
        use crate::{WalScanStop, WalScanStopReason};
        let manifest = clean_manifest();
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(20)),
            stopped: Some(WalScanStop {
                reason: WalScanStopReason::LsnGap,
                offset: 50,
            }),
        };
        let result = safe_start_from_manifest_and_scan(&manifest, &scan);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("SafeStart rejected"));
    }

    #[test]
    fn safe_start_rejects_ram_only_evidence() {
        let mut manifest = clean_manifest();
        manifest.required_wal_start_lsn = Lsn::new(100);
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(5)), // < required_wal_start_lsn
            stopped: None,
        };
        let result = safe_start_from_manifest_and_scan(&manifest, &scan);
        assert!(result.is_err());
    }

    #[test]
    fn safe_start_invariant_check_passes_clean_manifest() {
        let result = verify_safe_start_invariants(&clean_manifest(), &clean_scan());
        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.all_pass());
    }

    #[test]
    fn safe_start_invariant_check_fails_zero_snapshot() {
        let mut manifest = clean_manifest();
        manifest.snapshot_id = 0;
        let result = verify_safe_start_invariants(&manifest, &clean_scan());
        assert!(result.is_err());
    }
}
