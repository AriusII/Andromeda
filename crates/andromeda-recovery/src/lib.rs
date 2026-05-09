#![forbid(unsafe_code)]
#![doc = r#"
Boundary crate for Andromeda startup recovery and replay planning.

This crate owns recovery boundary contracts that do not depend on storage page
or WAL record internals. Storage keeps replay implementation during the
migration and imports these contracts through its compatibility surface.

C5 invariants:

- Recovery truth must come from durable WAL and durable artifacts, not RAM summaries.
- Visible commits after restart must be reconstructed only from durable commit evidence.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

mod coverage;
#[cfg(test)]
mod crash_recovery_matrix;
mod fast_start;
mod file_wal_report;
mod file_wal_startup;
mod forensic_start;
mod planning;
mod replay;
mod safe_start;
mod startup;
mod trace;
mod transaction_wal_bridge;
mod undo;
mod wal_replay;

use andromeda_error::{AndromedaError, AndromedaErrorKind};

/// Startup mode requested for recovery against durable evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMode {
    /// Requires a clean durable WAL scan before replay.
    FastStart,
    /// Allows replay through recoverable durable-tail truncation or corruption.
    SafeStart,
    /// Preserves evidence and prevents replay until a forensic report exists.
    ForensicStart,
}

pub use coverage::{WalCoverageEvidence, validate_wal_coverage};
pub use fast_start::{FastStartAcceptance, FastStartRejection, fast_start_from_manifest_and_scan};
pub use file_wal_report::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
    FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord, FileWalRecoveryReportV0,
    build_file_wal_recovery_report_v0, file_wal_recovery_boundary_kind,
    report_file_wal_recovery_from_scan_v0, report_file_wal_recovery_v0,
};
pub use file_wal_startup::{
    FileWalStartupRecoveryV0, plan_file_wal_startup_recovery_from_scan_v0,
    plan_file_wal_startup_recovery_v0, recover_from_file_wal,
    recovered_transaction_id_floor_from_records,
};
pub use forensic_start::{
    ForensicAnomaly, ForensicAnomalyKind, ForensicAnomalyReport, ForensicStartAcceptance,
    forensic_start_from_manifest_and_scan,
};
pub use planning::{
    ConceptualRedoPlan, PreRedoStorageFormatDecision, PreRedoStorageFormatGate,
    PreRedoStorageFormatRejection, RECOVERY_REQUIRED_STORAGE_FORMATS, RecoveryManifestView,
    RecoveryPlan, RecoveryTraceProjection, RedoRecordDecision, RedoRecordPlan,
    StorageFormatFingerprint,
};
pub use replay::{
    HeapRedoPageState, HeapRedoSlotState, IndexRebuildRequiredEvidence,
    ManifestSwitchRecoveryTrace, RecoveryReplayTarget, RecoveryWalReplayAdapter, ReplayContext,
    ReplayOutcome, ReplayResult, WalReplayReport, execute_redo_plan_with_adapter,
    replay_wal_record, replay_wal_record_result,
};
pub use safe_start::{
    SafeStartAcceptance, SafeStartInvariantReport, SafeStartTailDiscard,
    safe_start_from_manifest_and_scan, verify_safe_start_invariants,
};
pub use startup::{
    ObservedBoundary, StartupAcceptance, StartupAuditProjection, StartupDecision, StartupEvidence,
    StartupOutcome, StartupRejectionReason, decide_startup,
};
pub use trace::RecoveryTrace;
pub use transaction_wal_bridge::{
    CommitLogInvocationWal, DurableTransactionWalPrefix, TransactionReplayFromWalEvidence,
    TxReplayBridgeEvidence, map_durable_wal_prefix_to_tx_replay,
};
pub use undo::{UndoChain, UndoChainsBuilder, UndoOperation, UndoRecord};
pub use wal_replay::{
    execute_redo_plan, execute_redo_plan_into_context, replay_wal_from_lsn,
    replay_wal_from_lsn_into_context,
};

impl StartupMode {
    /// Returns `true` when this mode is allowed to replay records after a clean
    /// durable evidence scan.
    pub const fn permits_clean_replay(self) -> bool {
        matches!(self, Self::FastStart | Self::SafeStart)
    }

    /// Returns `true` when this mode requires a persisted forensic report.
    pub const fn requires_forensic_report(self) -> bool {
        matches!(self, Self::ForensicStart)
    }
}

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_mode_declares_replay_and_report_boundaries() {
        assert!(StartupMode::FastStart.permits_clean_replay());
        assert!(StartupMode::SafeStart.permits_clean_replay());
        assert!(!StartupMode::ForensicStart.permits_clean_replay());
        assert!(StartupMode::ForensicStart.requires_forensic_report());
    }
}
