use andromeda_core::AndromedaResult;
use std::path::Path;

use crate::{
    ConceptualRedoPlan, DatabaseManifest, DurableTransactionResume, DurableTransactionState,
    RecoveryPlan, RedoRecordDecision, RedoRecordPlan, StartupMode, WalRecord, WalScanStop,
    summarize_transactions_from_records,
};

use super::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
    FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord, FileWalRecoveryReportV0,
    scan::{is_forensic_scan_stop, scan_file_wal},
};

pub fn report_file_wal_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    manifest.validate()?;
    let disk_scan = scan_file_wal(path)?;
    let boundary_kind = recovery_boundary_kind(disk_scan.scan.stopped);
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
        let plan =
            RecoveryPlan::from_manifest_and_wal_scan(manifest, startup_mode, &disk_scan.scan)?;
        (
            recovery_report_replay_records(&plan),
            recovery_report_ignored_transactions(&plan),
            recovery_report_ignored_record_count(&plan),
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

fn recovery_boundary_kind(stop: Option<WalScanStop>) -> FileWalRecoveryBoundaryKind {
    if is_forensic_scan_stop(stop) {
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    } else if stop.is_some() {
        FileWalRecoveryBoundaryKind::RecoverableTail
    } else {
        FileWalRecoveryBoundaryKind::Clean
    }
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
        }
        DurableTransactionState::Open | DurableTransactionState::Incomplete => {
            Some(FileWalRecoveryIgnoredTransactionReason::Incomplete)
        }
        DurableTransactionState::Committed => None,
    }
}
