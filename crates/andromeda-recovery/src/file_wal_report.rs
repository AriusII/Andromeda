//! Runtime-free file-WAL recovery report DTOs.

use andromeda_core::TransactionId;
use andromeda_wal::{FileWalHeader, Lsn, WalRecordKind, WalScanStop, WalScanStopReason};

use crate::StartupMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWalRecoveryBoundaryKind {
    Clean,
    RecoverableTail,
    ForensicChainBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileWalRecoveryIgnoredTransactionReason {
    Incomplete,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWalRecoveryReplayRecord {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<TransactionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWalRecoveryIgnoredTransaction {
    pub transaction_id: TransactionId,
    pub reason: FileWalRecoveryIgnoredTransactionReason,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub record_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWalRecoveryReportV0 {
    pub startup_mode: StartupMode,
    pub header: FileWalHeader,
    pub physical_wal_bytes: u64,
    pub scanned_bytes: u64,
    pub durable_prefix_bytes: u64,
    pub durable_prefix_record_count: usize,
    pub durable_lsn: Lsn,
    pub scan_stop: Option<WalScanStop>,
    pub boundary_kind: FileWalRecoveryBoundaryKind,
    pub replay_records: Vec<FileWalRecoveryReplayRecord>,
    pub ignored_transactions: Vec<FileWalRecoveryIgnoredTransaction>,
    pub ignored_record_count: usize,
    pub forensic_required: bool,
}

impl FileWalRecoveryReportV0 {
    pub fn replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.replay_records.iter().map(|record| record.lsn)
    }

    pub fn ignored_transaction_ids(&self) -> impl Iterator<Item = TransactionId> + '_ {
        self.ignored_transactions
            .iter()
            .map(|transaction| transaction.transaction_id)
    }

    pub const fn has_recoverable_tail_boundary(&self) -> bool {
        matches!(
            self.boundary_kind,
            FileWalRecoveryBoundaryKind::RecoverableTail
        )
    }
}

pub fn file_wal_recovery_boundary_kind(stop: Option<WalScanStop>) -> FileWalRecoveryBoundaryKind {
    match stop.map(|stop| stop.reason) {
        Some(
            WalScanStopReason::LsnGap
            | WalScanStopReason::DuplicateOrReorderedLsn
            | WalScanStopReason::PreviousLsnMismatch,
        ) => FileWalRecoveryBoundaryKind::ForensicChainBreak,
        Some(_) => FileWalRecoveryBoundaryKind::RecoverableTail,
        None => FileWalRecoveryBoundaryKind::Clean,
    }
}
