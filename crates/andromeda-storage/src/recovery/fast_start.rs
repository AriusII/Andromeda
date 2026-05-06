//! FastStart recovery mode — assumes previous shutdown was clean.
//!
//! # Doctrine
//!
//! `FastStart` is the **strictest fast path**: it may only proceed when the
//! durable WAL scan observed a **clean boundary** (no truncated or corrupt tail,
//! no LSN chain breaks).  The rationale: "fast" means "no extra audit work",
//! which is only safe when we have already proven the WAL is intact.
//!
//! # Invariants (ALL must hold)
//!
//! 1. **Clean WAL boundary** — `wal_scan_stop` must be `None`.
//! 2. **Manifest validated** — cold-snapshot identity fields pass `validate()`.
//! 3. **Non-zero snapshot** — `mounted_snapshot_id != 0`.
//! 4. **Durable WAL covers anchor** — `last_durable_lsn >= required_wal_start_lsn`.
//!
//! # Performance Budget
//!
//! FastStart targets **< 100 ms** for small databases (< 1 GB) by skipping:
//! - Deep page-level CRC verification (deferred to background scrubber).
//! - Full transaction graph rebuilding (only the incomplete-tx discard list).
//! - Index consistency checks (assumed consistent after clean shutdown).
//!
//! # Relationship to `decide_startup`
//!
//! This module wraps [`super::startup::decide_startup`] with a FastStart-specific
//! validation guard and performance tracking.  Any startup acceptance proof
//! produced here carries `replay_allowed = true` and `observed_boundary = Clean`.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{DatabaseManifest, Lsn, WalScanResult};

use super::planning::StartupMode;
use super::startup::{
    ObservedBoundary, StartupAcceptance, StartupDecision, StartupEvidence, StartupOutcome,
    decide_startup,
};

/// Acceptance proof produced by a successful `FastStart`.
///
/// Carries the manifest recovery floor and the observed clean boundary.
/// A `FastStartAcceptance` value is proof that:
/// * The WAL scan was clean.
/// * The manifest was validated.
/// * Replay is authorised to proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastStartAcceptance {
    /// The LSN from which redo must begin (manifest's `required_wal_start_lsn`).
    pub redo_from_lsn: Lsn,
    /// The highest durable LSN observed by the WAL scan.
    pub last_durable_lsn: Lsn,
    /// Confirmed clean boundary (no tail corruption).
    pub observed_boundary: ObservedBoundary,
    /// Inner `StartupAcceptance` proof from `decide_startup`.
    pub inner: StartupAcceptance,
}

/// Error produced when `FastStart` conditions are not satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastStartRejection {
    /// Manifest failed `validate()`.
    ManifestInvalid,
    /// No cold snapshot is mounted.
    MissingColdSnapshot,
    /// The durable WAL does not cover the manifest's anchor LSN.
    WalDoesNotCoverAnchor,
    /// WAL scan exposed a recoverable tail; FastStart requires a clean scan.
    RecoverableTailNotAllowed,
    /// WAL scan exposed a chain break; ForensicStart is required.
    ForensicChainBreakDetected,
}

impl FastStartRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestInvalid => "fast_start_manifest_invalid",
            Self::MissingColdSnapshot => "fast_start_missing_cold_snapshot",
            Self::WalDoesNotCoverAnchor => "fast_start_wal_does_not_cover_anchor",
            Self::RecoverableTailNotAllowed => "fast_start_recoverable_tail_not_allowed",
            Self::ForensicChainBreakDetected => "fast_start_forensic_chain_break_detected",
        }
    }
}

/// Attempt a `FastStart` recovery from manifest and WAL scan evidence.
///
/// Returns [`FastStartAcceptance`] on success, or an `AndromedaError` on
/// any rejection.  The error message carries the `FastStartRejection` code.
///
/// # Preconditions
///
/// * `manifest.validate()` must pass.
/// * `scan.stopped` must be `None` (clean boundary).
/// * `scan.last_valid_lsn` must be `>= manifest.required_wal_start_lsn`.
pub fn fast_start_from_manifest_and_scan(
    manifest: &DatabaseManifest,
    scan: &WalScanResult,
) -> AndromedaResult<FastStartAcceptance> {
    let evidence = StartupEvidence::from_manifest_and_wal_scan(manifest, scan, false);
    let decision = decide_startup(StartupMode::FastStart, evidence);
    fast_start_from_decision(decision, manifest)
}

/// Convert an already-taken `StartupDecision` into a `FastStartAcceptance`.
///
/// Useful when the caller has already called `decide_startup` and wants the
/// typed FastStart proof without repeating the evidence construction.
pub fn fast_start_from_decision(
    decision: StartupDecision,
    manifest: &DatabaseManifest,
) -> AndromedaResult<FastStartAcceptance> {
    match decision.outcome {
        StartupOutcome::Accepted(inner) => Ok(FastStartAcceptance {
            redo_from_lsn: manifest.required_wal_start_lsn,
            last_durable_lsn: decision.evidence.last_durable_lsn,
            observed_boundary: inner.observed_boundary,
            inner,
        }),
        StartupOutcome::Rejected(reason) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "FastStart rejected ({}): {}",
                FastStartRejection::from_startup_reason_str(reason.as_static_str()),
                reason.as_static_str()
            ),
        )),
    }
}

impl FastStartRejection {
    fn from_startup_reason_str(s: &'static str) -> &'static str {
        // Map startup rejection reason strings to FastStart codes.
        match s {
            "manifest_not_validated" => "fast_start_manifest_invalid",
            "missing_cold_snapshot" => "fast_start_missing_cold_snapshot",
            "ram_only_evidence" => "fast_start_wal_does_not_cover_anchor",
            "fast_start_requires_clean_scan" => "fast_start_recoverable_tail_not_allowed",
            "forensic_handling_required" => "fast_start_forensic_chain_break_detected",
            _ => "fast_start_unknown_rejection",
        }
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
    fn fast_start_accepts_clean_wal_boundary() {
        let result = fast_start_from_manifest_and_scan(&clean_manifest(), &clean_scan());
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        let proof = result.unwrap();
        assert!(proof.inner.replay_allowed);
        assert_eq!(proof.observed_boundary, ObservedBoundary::Clean);
        assert_eq!(proof.redo_from_lsn, Lsn::new(10));
        assert_eq!(proof.last_durable_lsn, Lsn::new(20));
    }

    #[test]
    fn fast_start_rejects_missing_cold_snapshot() {
        let mut manifest = clean_manifest();
        manifest.snapshot_id = 0; // invalid — no snapshot
        // manifest.validate() will fail on snapshot_id == 0
        let scan = clean_scan();
        // evidence.manifest_validated will be false
        let evidence = StartupEvidence::from_manifest_and_wal_scan(&manifest, &scan, false);
        let decision = decide_startup(StartupMode::FastStart, evidence);
        let result = fast_start_from_decision(decision, &manifest);
        assert!(result.is_err());
    }

    #[test]
    fn fast_start_rejects_recoverable_tail() {
        use crate::{WalScanStop, WalScanStopReason};
        let manifest = clean_manifest();
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(20)),
            stopped: Some(WalScanStop {
                reason: WalScanStopReason::TruncatedRecord,
                offset: 100,
            }),
        };
        let result = fast_start_from_manifest_and_scan(&manifest, &scan);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("fast_start"));
    }

    #[test]
    fn fast_start_rejects_forensic_chain_break() {
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
        let result = fast_start_from_manifest_and_scan(&manifest, &scan);
        assert!(result.is_err());
    }

    #[test]
    fn fast_start_rejects_ram_only_evidence() {
        let mut manifest = clean_manifest();
        manifest.required_wal_start_lsn = Lsn::new(100);
        // scan.last_valid_lsn < required_wal_start_lsn → RAM-only evidence
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(5)),
            stopped: None,
        };
        let result = fast_start_from_manifest_and_scan(&manifest, &scan);
        assert!(result.is_err());
    }
}
