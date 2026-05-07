use crate::support::{TempWalPath, manifest, planned_decision};
use andromeda_core::TransactionId;
use andromeda_storage::write_ahead_log::codec::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION_V1,
};
use andromeda_storage::write_ahead_log::file::{
    FileWal, FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason,
    recover_from_file_wal, report_file_wal_recovery_v0, scan_file_wal,
};
use andromeda_storage::write_ahead_log::record::WalRecordKind;
use andromeda_storage::{DurableTransactionState, Lsn, RedoRecordDecision, StartupMode};

#[test]
fn file_wal_crash_after_mutation_before_commit_skips_incomplete_redo() {
    let temp = TempWalPath::new("crash-after-mutation");
    let transaction_id = TransactionId::new(701);

    let row_lsn = {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append_tx_begin(transaction_id).unwrap();
        let row_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(transaction_id), b"mutated")
            .unwrap();
        wal.flush_through(row_lsn).unwrap();
        row_lsn
    };

    let plan =
        recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path).unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert!(plan.has_incomplete_transactions());
    assert_eq!(
        planned_decision(&temp.path, row_lsn),
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(report.ignored_record_count, 1);
    assert_eq!(
        report.ignored_transactions[0].reason,
        FileWalRecoveryIgnoredTransactionReason::Incomplete
    );
}

#[test]
fn file_wal_commit_written_but_not_synced_is_hidden_on_reopen() {
    let temp = TempWalPath::new("commit-written-not-synced");
    let transaction_id = TransactionId::new(702);

    let (row_lsn, commit_lsn, append_bytes, durable_bytes) = {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append_tx_begin(transaction_id).unwrap();
        let row_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(transaction_id), b"pending")
            .unwrap();
        wal.flush_through(row_lsn).unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
        (row_lsn, commit_lsn, wal.append_bytes(), wal.durable_bytes())
    };

    assert!(append_bytes > durable_bytes);
    let disk_scan = scan_file_wal(&temp.path).unwrap();
    assert_eq!(disk_scan.durable_lsn, row_lsn);
    assert!(disk_scan.physical_wal_bytes > disk_scan.scanned_bytes);

    let reopened = FileWal::open(&temp.path).unwrap();
    assert_eq!(reopened.last_lsn(), Some(row_lsn));
    assert!(
        reopened
            .records()
            .iter()
            .all(|record| record.header.lsn < commit_lsn)
    );
    assert_eq!(
        planned_decision(&temp.path, row_lsn),
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert_eq!(report.durable_lsn, row_lsn);
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(
        report
            .ignored_transactions
            .iter()
            .map(|ignored| ignored.reason)
            .collect::<Vec<_>>(),
        vec![FileWalRecoveryIgnoredTransactionReason::Incomplete]
    );
}

#[test]
fn file_wal_commit_synced_before_completion_is_visible_to_redo_plan() {
    let temp = TempWalPath::new("commit-synced");
    let transaction_id = TransactionId::new(703);

    let row_lsn = {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append_tx_begin(transaction_id).unwrap();
        let row_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(transaction_id), b"complete")
            .unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
        wal.flush_through(commit_lsn).unwrap();
        row_lsn
    };

    let disk_scan = scan_file_wal(&temp.path).unwrap();
    assert_eq!(disk_scan.header.format_version, WAL_FORMAT_VERSION_V1);
    assert_eq!(disk_scan.header.byte_order, WAL_BYTE_ORDER_LITTLE_ENDIAN);

    let plan =
        recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path).unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![row_lsn]);
    assert_eq!(
        plan.transaction_evidence[0].state,
        DurableTransactionState::Committed
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert!(!report.forensic_required);
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), vec![row_lsn]);
    assert_eq!(report.replay_records[0].kind, WalRecordKind::RowUpdate);
    assert!(report.ignored_transactions.is_empty());
}

#[test]
fn file_wal_rollback_durable_is_invisible_to_redo_report() {
    let temp = TempWalPath::new("rollback-durable");
    let transaction_id = TransactionId::new(706);

    let row_lsn = {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append_tx_begin(transaction_id).unwrap();
        let row_lsn = wal
            .append_payload(
                WalRecordKind::RowDelete,
                Some(transaction_id),
                b"rolled-back",
            )
            .unwrap();
        let rollback_lsn = wal.append_tx_rollback(transaction_id).unwrap();
        wal.flush_through(rollback_lsn).unwrap();
        row_lsn
    };

    let plan =
        recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path).unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(
        planned_decision(&temp.path, row_lsn),
        RedoRecordDecision::SkipRolledBackTransaction
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(report.ignored_record_count, 1);
    assert_eq!(
        report.ignored_transactions[0].reason,
        FileWalRecoveryIgnoredTransactionReason::RolledBack
    );
    assert_eq!(
        report.ignored_transactions[0].transaction_id,
        transaction_id
    );
}
