//! Runtime-free file-WAL recovery report DTOs.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_wal::{
    DurableTransactionResume, DurableTransactionState, FileWalDiskScan, FileWalHeader, Lsn,
    WalRecord, WalRecordKind, WalScanStop, WalScanStopReason, summarize_transactions_from_records,
};

use crate::{ConceptualRedoPlan, RedoRecordDecision, RedoRecordPlan, StartupMode};

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

/// Builds the portable V0 file-WAL recovery report from a durable disk scan
/// and optional conceptual redo plan.
///
/// Storage remains responsible for opening/scanning the file and building the
/// plan from a concrete manifest. This owner function partitions replayable
/// records, ignored transaction evidence, durable byte counts, and forensic
/// chain-break boundaries without depending on storage page/heap/index state.
pub fn build_file_wal_recovery_report_v0(
    startup_mode: StartupMode,
    disk_scan: &FileWalDiskScan,
    redo_plan: Option<&ConceptualRedoPlan>,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    let boundary_kind = file_wal_recovery_boundary_kind(disk_scan.scan.stopped);
    let forensic_required = matches!(
        boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );

    let (replay_records, ignored_transactions, ignored_record_count) = if forensic_required {
        (
            Vec::new(),
            ignored_transactions_from_prefix(&disk_scan.scan.records),
            0,
        )
    } else {
        let Some(plan) = redo_plan else {
            return Err(recovery_error(
                "file-WAL recovery report requires a redo plan unless the scan hit a forensic chain break",
            ));
        };
        (
            recovery_report_replay_records(plan),
            recovery_report_ignored_transactions(plan),
            recovery_report_ignored_record_count(plan),
        )
    };

    Ok(FileWalRecoveryReportV0 {
        startup_mode,
        header: disk_scan.header,
        physical_wal_bytes: disk_scan.physical_wal_bytes,
        scanned_bytes: disk_scan.scanned_bytes,
        durable_prefix_bytes: disk_scan.durable_bytes,
        durable_prefix_record_count: disk_scan.scan.records.len(),
        durable_lsn: disk_scan.durable_lsn,
        scan_stop: disk_scan.scan.stopped,
        boundary_kind,
        replay_records,
        ignored_transactions,
        ignored_record_count,
        forensic_required,
    })
}

fn recovery_report_replay_records(plan: &ConceptualRedoPlan) -> Vec<FileWalRecoveryReplayRecord> {
    plan.records
        .iter()
        .filter(|record| record.should_replay())
        .map(report_replay_record_from_redo_record)
        .collect()
}

fn report_replay_record_from_redo_record(record: &RedoRecordPlan) -> FileWalRecoveryReplayRecord {
    FileWalRecoveryReplayRecord {
        lsn: record.lsn,
        kind: record.kind,
        transaction_id: record.transaction_id,
    }
}

fn recovery_report_ignored_transactions(
    plan: &ConceptualRedoPlan,
) -> Vec<FileWalRecoveryIgnoredTransaction> {
    plan.transaction_evidence
        .iter()
        .filter_map(ignored_transaction_from_summary)
        .collect()
}

fn recovery_report_ignored_record_count(plan: &ConceptualRedoPlan) -> usize {
    plan.records
        .iter()
        .filter(|record| {
            matches!(
                record.decision,
                RedoRecordDecision::SkipIncompleteTransaction
                    | RedoRecordDecision::SkipRolledBackTransaction
            )
        })
        .count()
}

fn ignored_transactions_from_prefix(
    records: &[WalRecord],
) -> Vec<FileWalRecoveryIgnoredTransaction> {
    summarize_transactions_from_records(records)
        .into_iter()
        .filter_map(|summary| ignored_transaction_from_summary(&summary))
        .collect()
}

fn ignored_transaction_from_summary(
    summary: &DurableTransactionResume,
) -> Option<FileWalRecoveryIgnoredTransaction> {
    ignored_transaction_reason(summary.state).map(|reason| FileWalRecoveryIgnoredTransaction {
        transaction_id: summary.transaction_id,
        reason,
        first_lsn: summary.first_lsn,
        last_lsn: summary.last_lsn,
        record_count: summary.record_count,
    })
}

fn ignored_transaction_reason(
    state: DurableTransactionState,
) -> Option<FileWalRecoveryIgnoredTransactionReason> {
    match state {
        DurableTransactionState::RolledBack => {
            Some(FileWalRecoveryIgnoredTransactionReason::RolledBack)
        },
        DurableTransactionState::Open | DurableTransactionState::Incomplete => {
            Some(FileWalRecoveryIgnoredTransactionReason::Incomplete)
        },
        DurableTransactionState::Committed => None,
    }
}

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
