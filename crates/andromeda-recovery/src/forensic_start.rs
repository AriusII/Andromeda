//! ForensicStart recovery mode proof and WAL scan anomaly classification.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{Lsn, WalScanResult, WalScanStop, WalScanStopReason};

use crate::{
    ObservedBoundary, RecoveryManifestView, StartupAcceptance, StartupDecision, StartupEvidence,
    StartupMode, StartupOutcome, decide_startup,
    forensic_report::ForensicStartReport,
    forensic_startup_gate::{ApplicationSurfaceDisposition, block_application_surface},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicStartAcceptance {
    pub redo_from_lsn: Lsn,
    pub last_durable_lsn: Lsn,
    pub observed_boundary: ObservedBoundary,
    pub anomaly_report: ForensicAnomalyReport,
    pub inner: StartupAcceptance,
    /// Attached forensic report, present when produced via
    /// [`forensic_start_with_report`].
    pub report: Option<ForensicStartReport>,
}

impl ForensicStartAcceptance {
    pub const fn replay_allowed(&self) -> bool {
        false
    }

    /// Returns the typed Application Surface disposition for this acceptance.
    ///
    /// ForensicStart acceptance is evidence-preserving only; it never permits
    /// Application-plane ingress.
    pub const fn application_surface_disposition(&self) -> ApplicationSurfaceDisposition {
        ApplicationSurfaceDisposition::BlockForensic
    }

    pub fn has_chain_break(&self) -> bool {
        self.anomaly_report.has_chain_break()
    }

    /// Attach a [`ForensicStartReport`] to this acceptance proof.
    pub fn with_report(mut self, report: ForensicStartReport) -> Self {
        self.report = Some(report);
        self
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

pub(crate) fn forensic_start_from_manifest_and_scan(
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
                report: None,
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

/// Sibling of [`forensic_start_from_manifest_and_scan`] that also requires a
/// fully validated [`ForensicStartReport`].
///
/// This function:
/// 1. Rejects `report` if `report.all_sections_validated()` returns `false`.
/// 2. Runs the standard ForensicStart decision (equivalent to calling
///    [`forensic_start_from_manifest_and_scan`] with `forensic_report_attached =
///    true`).
/// 3. Performs a C5 gate consistency assertion: confirms that
///    [`block_application_surface(ForensicStart)`] returns `true` (this is an
///    invariant, not a recoverable check).
/// 4. Attaches the report to the returned [`ForensicStartAcceptance`] via
///    [`ForensicStartAcceptance::with_report`].
///
/// [`block_application_surface(ForensicStart)`]: crate::forensic_startup_gate::block_application_surface
pub fn forensic_start_with_report(
    manifest: &impl RecoveryManifestView,
    scan: &WalScanResult,
    report: ForensicStartReport,
) -> AndromedaResult<ForensicStartAcceptance> {
    if !report.all_sections_validated() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: all_sections_validated() returned false; \
             every section must pass validation before the report can be presented",
        ));
    }
    validate_forensic_report_binding(manifest, scan, &report)?;

    // Gate consistency: ForensicStart MUST always block the Application Surface.
    // This is a type-level invariant, not a recoverable error.
    debug_assert!(
        block_application_surface(StartupMode::ForensicStart),
        "C5 gate invariant violated: ForensicStart must block Application Surface"
    );

    let acceptance = forensic_start_from_manifest_and_scan(manifest, scan, true)?;
    Ok(acceptance.with_report(report))
}

fn validate_forensic_report_binding(
    manifest: &impl RecoveryManifestView,
    scan: &WalScanResult,
    report: &ForensicStartReport,
) -> AndromedaResult<()> {
    if report.snapshot.snapshot_id != manifest.mounted_snapshot_id() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: snapshot id must match mounted manifest snapshot",
        ));
    }
    if report.wal.coverage_start_lsn != manifest.required_wal_start_lsn().get() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: WAL coverage start must match manifest required WAL start",
        ));
    }
    let last_durable_lsn = scan.last_valid_lsn.unwrap_or(Lsn::ZERO).get();
    if report.wal.coverage_end_lsn != last_durable_lsn {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: WAL coverage end must match scan last durable LSN",
        ));
    }
    if report.wal.scan_complete != scan.stopped.is_none() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: WAL scan_complete must match scan stop state",
        ));
    }

    match (scan.stopped, &report.wal.anomaly) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: anomaly report present for clean WAL scan",
        )),
        (Some(stop), Some(anomaly)) if anomaly.scan_stop == Some(stop) => Ok(()),
        (Some(_), _) => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "ForensicStart report rejected: anomaly report must bind to WAL scan stop",
        )),
    }
}
