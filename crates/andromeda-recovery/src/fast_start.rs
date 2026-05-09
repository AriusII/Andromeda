//! FastStart recovery mode proof.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{Lsn, WalScanResult};

use crate::{
    ObservedBoundary, RecoveryManifestView, StartupAcceptance, StartupDecision, StartupEvidence,
    StartupMode, StartupOutcome, decide_startup,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastStartAcceptance {
    pub redo_from_lsn: Lsn,
    pub last_durable_lsn: Lsn,
    pub observed_boundary: ObservedBoundary,
    pub inner: StartupAcceptance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastStartRejection {
    ManifestInvalid,
    MissingColdSnapshot,
    WalDoesNotCoverAnchor,
    RecoverableTailNotAllowed,
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

    fn from_startup_reason_str(s: &'static str) -> &'static str {
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

pub fn fast_start_from_manifest_and_scan(
    manifest: &impl RecoveryManifestView,
    scan: &WalScanResult,
) -> AndromedaResult<FastStartAcceptance> {
    let evidence = StartupEvidence::from_manifest_and_wal_scan(manifest, scan, false);
    let decision = decide_startup(StartupMode::FastStart, evidence);
    fast_start_from_decision(decision)
}

pub fn fast_start_from_decision(decision: StartupDecision) -> AndromedaResult<FastStartAcceptance> {
    match decision.outcome {
        StartupOutcome::Accepted(inner) => Ok(FastStartAcceptance {
            redo_from_lsn: decision.evidence.required_wal_start_lsn,
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
