//! ForensicStart recovery mode proof and WAL scan anomaly classification.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{Lsn, WalScanResult, WalScanStop, WalScanStopReason};

use crate::{
    ObservedBoundary, RecoveryManifestView, StartupAcceptance, StartupDecision, StartupEvidence,
    StartupMode, StartupOutcome, decide_startup,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicStartAcceptance {
    pub redo_from_lsn: Lsn,
    pub last_durable_lsn: Lsn,
    pub observed_boundary: ObservedBoundary,
    pub anomaly_report: ForensicAnomalyReport,
    pub inner: StartupAcceptance,
}

impl ForensicStartAcceptance {
    pub const fn replay_allowed(&self) -> bool {
        false
    }

    pub fn has_chain_break(&self) -> bool {
        self.anomaly_report.has_chain_break()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicAnomalyReport {
    pub anomalies: Vec<ForensicAnomaly>,
    pub scan_stop: Option<WalScanStop>,
}

impl ForensicAnomalyReport {
    pub fn has_chain_break(&self) -> bool {
        self.anomalies
            .iter()
            .any(|a| matches!(a.kind, ForensicAnomalyKind::LsnChainBreak { .. }))
    }

    pub fn has_recoverable_tail(&self) -> bool {
        self.anomalies.iter().any(|a| {
            matches!(
                a.kind,
                ForensicAnomalyKind::RecoverableTailTruncation { .. }
            )
        })
    }

    pub fn is_clean(&self) -> bool {
        self.anomalies.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicAnomaly {
    pub kind: ForensicAnomalyKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForensicAnomalyKind {
    LsnChainBreak { offset: u64 },
    RecoverableTailTruncation { offset: u64 },
    ChecksumFailure { offset: u64 },
    UnexpectedRecordType { offset: u64 },
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

pub fn forensic_start_from_manifest_and_scan(
    manifest: &impl RecoveryManifestView,
    scan: &WalScanResult,
    forensic_report_attached: bool,
) -> AndromedaResult<ForensicStartAcceptance> {
    let evidence =
        StartupEvidence::from_manifest_and_wal_scan(manifest, scan, forensic_report_attached);
    let decision = decide_startup(StartupMode::ForensicStart, evidence);
    forensic_start_from_decision(decision, scan)
}

pub fn forensic_start_from_decision(
    decision: StartupDecision,
    scan: &WalScanResult,
) -> AndromedaResult<ForensicStartAcceptance> {
    match decision.outcome {
        StartupOutcome::Accepted(inner) => {
            let anomalies = scan
                .stopped
                .map(|stop| vec![classify_scan_stop(stop)])
                .unwrap_or_default();
            let anomaly_report = ForensicAnomalyReport {
                anomalies,
                scan_stop: scan.stopped,
            };

            Ok(ForensicStartAcceptance {
                redo_from_lsn: decision.evidence.required_wal_start_lsn,
                last_durable_lsn: decision.evidence.last_durable_lsn,
                observed_boundary: inner.observed_boundary,
                anomaly_report,
                inner,
            })
        },
        StartupOutcome::Rejected(reason) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "ForensicStart rejected ({}): attach a forensic report before proceeding",
                reason.as_static_str()
            ),
        )),
    }
}

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
        },
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
