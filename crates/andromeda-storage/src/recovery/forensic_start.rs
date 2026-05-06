//! ForensicStart recovery mode — deep inspection without mutation.
//!
//! # Doctrine
//!
//! `ForensicStart` is the **read-only forensic investigation path**.  It is the
//! only startup mode that accepts a WAL chain break inside the durable prefix.
//! When active, **no WAL records are applied** (`replay_allowed = false`):
//! the database may be inspected but the cold snapshot must not be mutated.
//!
//! # When to Use ForensicStart
//!
//! * A WAL chain break was detected (LSN gap, duplicate/reordered LSN, or
//!   `previous_lsn` mismatch).
//! * An operator wants to inspect pre-crash state without modifying it.
//! * Automated anomaly detection has triggered a forensic hold.
//!
//! # Prerequisites
//!
//! `ForensicStart` **requires a forensic report to be attached** before it may
//! proceed.  A forensic report is a durable artefact (e.g. a
//! `FileWalRecoveryReportV0`) that captures the observed anomalies and serves
//! as the authorisation for the read-only mount.  Without it, `ForensicStart`
//! is rejected to prevent accidental bypassing of safety checks.
//!
//! # Output
//!
//! On acceptance, `ForensicStart` produces:
//! * A [`ForensicStartAcceptance`] proof (replay disabled).
//! * A [`ForensicAnomalyReport`] listing detected anomalies from the WAL scan.
//!
//! # Relationship to `decide_startup`
//!
//! This module wraps [`super::startup::decide_startup`] with a ForensicStart-
//! specific proof type and an anomaly classification layer.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{DatabaseManifest, Lsn, WalScanResult, WalScanStop, WalScanStopReason};

use super::planning::StartupMode;
use super::startup::{
    ObservedBoundary, StartupAcceptance, StartupDecision, StartupEvidence, StartupOutcome,
    decide_startup,
};

// ─── Forensic proof ────────────────────────────────────────────────────────

/// Acceptance proof produced by a successful `ForensicStart`.
///
/// `replay_allowed` is always `false` here: the engine may inspect but not mutate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicStartAcceptance {
    /// The LSN from which replay *would have* begun. Not applied.
    pub redo_from_lsn: Lsn,
    /// The highest durable LSN observed by the scan (boundary of truth).
    pub last_durable_lsn: Lsn,
    /// WAL boundary classification.
    pub observed_boundary: ObservedBoundary,
    /// Anomalies detected during the WAL scan.
    pub anomaly_report: ForensicAnomalyReport,
    /// Inner `StartupAcceptance` proof from `decide_startup`.
    /// `inner.replay_allowed` is always `false` in ForensicStart.
    pub inner: StartupAcceptance,
}

impl ForensicStartAcceptance {
    /// ForensicStart always disables replay.
    pub const fn replay_allowed(&self) -> bool {
        false
    }

    /// Returns `true` if the WAL scan observed a chain break.
    pub fn has_chain_break(&self) -> bool {
        self.anomaly_report.has_chain_break()
    }
}

// ─── Anomaly classification ────────────────────────────────────────────────

/// Classified anomalies found during the forensic WAL scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicAnomalyReport {
    /// Anomalies extracted from the WAL scan stop record.
    pub anomalies: Vec<ForensicAnomaly>,
    /// The WAL scan stop record, if any (source of the anomaly classification).
    pub scan_stop: Option<WalScanStop>,
}

impl ForensicAnomalyReport {
    /// Returns `true` if a chain break was detected inside the durable prefix.
    pub fn has_chain_break(&self) -> bool {
        self.anomalies
            .iter()
            .any(|a| matches!(a.kind, ForensicAnomalyKind::LsnChainBreak { .. }))
    }

    /// Returns `true` if a recoverable tail truncation was detected.
    pub fn has_recoverable_tail(&self) -> bool {
        self.anomalies.iter().any(|a| {
            matches!(
                a.kind,
                ForensicAnomalyKind::RecoverableTailTruncation { .. }
            )
        })
    }

    /// Returns `true` when no anomalies were detected (clean scan).
    pub fn is_clean(&self) -> bool {
        self.anomalies.is_empty()
    }
}

/// A single forensic anomaly classified from the WAL scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicAnomaly {
    /// Machine-readable anomaly code.
    pub kind: ForensicAnomalyKind,
    /// Human-readable description for operator inspection.
    pub detail: String,
}

/// Kinds of anomalies that `ForensicStart` can detect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForensicAnomalyKind {
    /// A gap or ordering anomaly in the durable LSN chain.
    LsnChainBreak {
        /// WAL byte offset where the break was detected.
        offset: u64,
    },
    /// A recoverable truncation at the WAL tail (last record incomplete).
    RecoverableTailTruncation {
        /// WAL byte offset of the truncation.
        offset: u64,
    },
    /// A checksum failure on a WAL record header or payload.
    ChecksumFailure { offset: u64 },
    /// An unexpected WAL record type at the scan position.
    UnexpectedRecordType { offset: u64 },
    /// The WAL tail contained valid bytes but no complete record.
    EmptyTail,
}

impl ForensicAnomalyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LsnChainBreak { .. } => "lsn_chain_break",
            Self::RecoverableTailTruncation { .. } => "recoverable_tail_truncation",
            Self::ChecksumFailure { .. } => "checksum_failure",
            Self::UnexpectedRecordType { .. } => "unexpected_record_type",
            Self::EmptyTail => "empty_tail",
        }
    }
}

// ─── Anomaly classification from WalScanStop ──────────────────────────────

fn classify_scan_stop(stop: WalScanStop) -> ForensicAnomaly {
    let offset = stop.offset as u64;
    match stop.reason {
        WalScanStopReason::LsnGap
        | WalScanStopReason::DuplicateOrReorderedLsn
        | WalScanStopReason::PreviousLsnMismatch => ForensicAnomaly {
            kind: ForensicAnomalyKind::LsnChainBreak { offset },
            detail: format!(
                "WAL LSN chain break detected at offset {offset}: {:?}",
                stop.reason
            ),
        },
        WalScanStopReason::TruncatedHeader | WalScanStopReason::TruncatedRecord => {
            ForensicAnomaly {
                kind: ForensicAnomalyKind::RecoverableTailTruncation { offset },
                detail: format!("WAL tail truncated at offset {offset}: {:?}", stop.reason),
            }
        }
        WalScanStopReason::CorruptRecord => ForensicAnomaly {
            kind: ForensicAnomalyKind::ChecksumFailure { offset },
            detail: format!("WAL record checksum failure at offset {offset}"),
        },
        WalScanStopReason::CorruptHeader => ForensicAnomaly {
            kind: ForensicAnomalyKind::UnexpectedRecordType { offset },
            detail: format!("invalid WAL record header at offset {offset}"),
        },
    }
}

// ─── ForensicStart entry points ───────────────────────────────────────────

/// Attempt a `ForensicStart` from manifest and WAL scan evidence.
///
/// Requires `forensic_report_attached = true`.  Accepts any WAL boundary
/// condition (clean, recoverable tail, or chain break) because forensic mode
/// never applies WAL records.
///
/// # Preconditions
///
/// * `manifest.validate()` must pass.
/// * A forensic report must be prepared and `forensic_report_attached = true`.
pub fn forensic_start_from_manifest_and_scan(
    manifest: &DatabaseManifest,
    scan: &WalScanResult,
    forensic_report_attached: bool,
) -> AndromedaResult<ForensicStartAcceptance> {
    let evidence =
        StartupEvidence::from_manifest_and_wal_scan(manifest, scan, forensic_report_attached);
    let decision = decide_startup(StartupMode::ForensicStart, evidence);
    forensic_start_from_decision(decision, manifest, scan)
}

/// Convert an already-taken `StartupDecision` into a `ForensicStartAcceptance`.
pub fn forensic_start_from_decision(
    decision: StartupDecision,
    manifest: &DatabaseManifest,
    scan: &WalScanResult,
) -> AndromedaResult<ForensicStartAcceptance> {
    match decision.outcome {
        StartupOutcome::Accepted(inner) => {
            // Build anomaly report from scan stop.
            let anomalies = scan
                .stopped
                .map(|stop| vec![classify_scan_stop(stop)])
                .unwrap_or_default();
            let anomaly_report = ForensicAnomalyReport {
                anomalies,
                scan_stop: scan.stopped,
            };

            Ok(ForensicStartAcceptance {
                redo_from_lsn: manifest.required_wal_start_lsn,
                last_durable_lsn: decision.evidence.last_durable_lsn,
                observed_boundary: inner.observed_boundary,
                anomaly_report,
                inner,
            })
        }
        StartupOutcome::Rejected(reason) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "ForensicStart rejected ({}): attach a forensic report before proceeding",
                reason.as_static_str()
            ),
        )),
    }
}

// ─── Unit Tests ────────────────────────────────────────────────────────────

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
    fn forensic_start_accepts_with_report_clean_wal() {
        let result = forensic_start_from_manifest_and_scan(&clean_manifest(), &clean_scan(), true);
        assert!(result.is_ok());
        let proof = result.unwrap();
        assert!(
            !proof.replay_allowed(),
            "ForensicStart must never allow replay"
        );
        assert!(proof.anomaly_report.is_clean());
    }

    #[test]
    fn forensic_start_rejects_without_report() {
        let result = forensic_start_from_manifest_and_scan(&clean_manifest(), &clean_scan(), false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("ForensicStart rejected")
        );
    }

    #[test]
    fn forensic_start_accepts_with_chain_break_and_report() {
        use crate::{WalScanStop, WalScanStopReason};
        let manifest = clean_manifest();
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(20)),
            stopped: Some(WalScanStop {
                reason: WalScanStopReason::LsnGap,
                offset: 42,
            }),
        };
        let result = forensic_start_from_manifest_and_scan(&manifest, &scan, true);
        assert!(
            result.is_ok(),
            "ForensicStart must accept chain breaks with report"
        );
        let proof = result.unwrap();
        assert!(!proof.replay_allowed());
        assert!(proof.has_chain_break());
        assert_eq!(
            proof.anomaly_report.anomalies[0].kind,
            ForensicAnomalyKind::LsnChainBreak { offset: 42 }
        );
    }

    #[test]
    fn forensic_start_classifies_recoverable_tail_correctly() {
        use crate::{WalScanStop, WalScanStopReason};
        let manifest = clean_manifest();
        let scan = WalScanResult {
            records: vec![],
            valid_bytes: 0,
            last_valid_lsn: Some(Lsn::new(20)),
            stopped: Some(WalScanStop {
                reason: WalScanStopReason::TruncatedRecord,
                offset: 99,
            }),
        };
        let result = forensic_start_from_manifest_and_scan(&manifest, &scan, true);
        assert!(result.is_ok());
        let proof = result.unwrap();
        assert!(!proof.has_chain_break());
        assert!(proof.anomaly_report.has_recoverable_tail());
    }

    #[test]
    fn forensic_start_preserves_forensic_report_in_acceptance() {
        let result = forensic_start_from_manifest_and_scan(&clean_manifest(), &clean_scan(), true);
        let proof = result.unwrap();
        assert!(
            proof.inner.forensic_report_preserved,
            "ForensicStart must preserve forensic report"
        );
    }
}
