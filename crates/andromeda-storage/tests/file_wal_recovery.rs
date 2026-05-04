use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::write_ahead_log::codec::{
    encode_wal_record, WalScanStopReason, WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION_V1,
};
use andromeda_storage::write_ahead_log::file::{
    recover_from_file_wal, report_file_wal_recovery_v0, scan_file_wal, FileWal,
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason, FILE_WAL_HEADER_LEN,
};
use andromeda_storage::write_ahead_log::record::{WalRecord, WalRecordKind};
use andromeda_storage::{
    DatabaseManifest, DurableTransactionState, Lsn, RedoRecordDecision, StartupMode,
};
use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct TempWalPath {
    path: PathBuf,
}

impl TempWalPath {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "andromeda-storage-{name}-{}-{suffix}.wal",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self { path }
    }
}

impl Drop for TempWalPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

fn planned_decision(path: &Path, lsn: Lsn) -> RedoRecordDecision {
    recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, path)
        .unwrap()
        .records
        .into_iter()
        .find(|record| record.lsn == lsn)
        .expect("planned record should exist")
        .decision
}

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
    assert!(reopened
        .records()
        .iter()
        .all(|record| record.header.lsn < commit_lsn));
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

#[test]
fn file_wal_truncated_and_corrupt_prefix_boundaries_keep_recoverable_prefix() {
    for corrupt_tail in [false, true] {
        let temp = TempWalPath::new(if corrupt_tail {
            "corrupt-prefix"
        } else {
            "truncated-prefix"
        });
        let first_row_lsn = write_two_committed_transactions(&temp.path);

        if corrupt_tail {
            let len = std::fs::metadata(&temp.path).unwrap().len();
            let mut file = OpenOptions::new().write(true).open(&temp.path).unwrap();
            file.seek(SeekFrom::Start(len - 1)).unwrap();
            file.write_all(&[0xa5]).unwrap();
        } else {
            let len = std::fs::metadata(&temp.path).unwrap().len();
            OpenOptions::new()
                .write(true)
                .open(&temp.path)
                .unwrap()
                .set_len(len - 1)
                .unwrap();
        }

        let disk_scan = scan_file_wal(&temp.path).unwrap();
        let stop_reason = disk_scan.scan.stopped.unwrap().reason;
        if corrupt_tail {
            assert!(matches!(
                stop_reason,
                WalScanStopReason::CorruptHeader | WalScanStopReason::CorruptRecord
            ));
        } else {
            assert!(matches!(
                stop_reason,
                WalScanStopReason::TruncatedHeader | WalScanStopReason::TruncatedRecord
            ));
        }

        let plan =
            recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
                .unwrap();
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![first_row_lsn]);
        assert_eq!(plan.wal_scan_stop().unwrap().reason, stop_reason);

        let report =
            report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
                .unwrap();
        assert_eq!(
            report.boundary_kind,
            FileWalRecoveryBoundaryKind::RecoverableTail
        );
        assert!(report.has_recoverable_tail_boundary());
        assert!(!report.forensic_required);
        assert_eq!(report.scan_stop.unwrap().reason, stop_reason);
        assert_eq!(
            report.replay_lsns().collect::<Vec<_>>(),
            vec![first_row_lsn]
        );
    }
}

#[test]
fn file_wal_lsn_gap_is_forensic_rejection() {
    let temp = TempWalPath::new("lsn-gap");
    let first_record = WalRecord::from_parts(
        WalRecordKind::PageAllocate,
        Lsn::new(1),
        None,
        None,
        b"first",
    )
    .unwrap();
    let first_len = encode_wal_record(&first_record).unwrap().len() as u64;

    {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append(first_record).unwrap();
        wal.append_payload(WalRecordKind::PageFormat, None, b"second")
            .unwrap();
        wal.flush_all().unwrap();
    }

    let gap_record = WalRecord::from_parts(
        WalRecordKind::PageFormat,
        Lsn::new(3),
        Some(Lsn::new(1)),
        None,
        b"second",
    )
    .unwrap();
    let mut file = OpenOptions::new().write(true).open(&temp.path).unwrap();
    file.seek(SeekFrom::Start(FILE_WAL_HEADER_LEN as u64 + first_len))
        .unwrap();
    file.write_all(&encode_wal_record(&gap_record).unwrap())
        .unwrap();

    let disk_scan = scan_file_wal(&temp.path).unwrap();
    assert_eq!(
        disk_scan.scan.stopped.unwrap().reason,
        WalScanStopReason::LsnGap
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(
        report.boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );
    assert!(report.forensic_required);
    assert_eq!(report.durable_lsn, Lsn::new(1));
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());

    assert_eq!(
        recover_from_file_wal(
            &manifest(Lsn::new(1)),
            StartupMode::ForensicStart,
            &temp.path
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        FileWal::open(&temp.path).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn file_wal_previous_lsn_mismatch_is_forensic_report_and_rejection() {
    let temp = TempWalPath::new("previous-mismatch");
    let first_record = WalRecord::from_parts(
        WalRecordKind::PageAllocate,
        Lsn::new(1),
        None,
        None,
        b"first",
    )
    .unwrap();
    let first_len = encode_wal_record(&first_record).unwrap().len() as u64;

    {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append(first_record).unwrap();
        wal.append_payload(WalRecordKind::PageFormat, None, b"second")
            .unwrap();
        wal.flush_all().unwrap();
    }

    let mismatch_record = WalRecord::from_parts(
        WalRecordKind::PageFormat,
        Lsn::new(2),
        None,
        None,
        b"second",
    )
    .unwrap();
    let mut file = OpenOptions::new().write(true).open(&temp.path).unwrap();
    file.seek(SeekFrom::Start(FILE_WAL_HEADER_LEN as u64 + first_len))
        .unwrap();
    file.write_all(&encode_wal_record(&mismatch_record).unwrap())
        .unwrap();

    let disk_scan = scan_file_wal(&temp.path).unwrap();
    assert_eq!(
        disk_scan.scan.stopped.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(
        report.boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );
    assert!(report.forensic_required);
    assert_eq!(
        report.scan_stop.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );

    assert_eq!(
        recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        FileWal::open(&temp.path).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn file_wal_recovery_report_partitions_replay_and_ignored_transactions() {
    let temp = TempWalPath::new("recovery-report-partitions");
    let committed_tx = TransactionId::new(707);
    let rolled_back_tx = TransactionId::new(708);
    let incomplete_tx = TransactionId::new(709);

    let committed_lsn = {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append_tx_begin(committed_tx).unwrap();
        let committed_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"commit")
            .unwrap();
        wal.append_tx_commit(committed_tx).unwrap();

        wal.append_tx_begin(rolled_back_tx).unwrap();
        wal.append_payload(WalRecordKind::RowUpdate, Some(rolled_back_tx), b"rollback")
            .unwrap();
        wal.append_tx_rollback(rolled_back_tx).unwrap();

        wal.append_tx_begin(incomplete_tx).unwrap();
        wal.append_payload(WalRecordKind::RowDelete, Some(incomplete_tx), b"incomplete")
            .unwrap();
        wal.flush_all().unwrap();
        committed_lsn
    };

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();

    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert_eq!(report.durable_prefix_record_count, 8);
    assert_eq!(
        report.replay_lsns().collect::<Vec<_>>(),
        vec![committed_lsn]
    );
    assert_eq!(report.ignored_record_count, 2);
    assert_eq!(
        report.ignored_transaction_ids().collect::<Vec<_>>(),
        vec![rolled_back_tx, incomplete_tx]
    );
    assert_eq!(
        report
            .ignored_transactions
            .iter()
            .map(|ignored| ignored.reason)
            .collect::<Vec<_>>(),
        vec![
            FileWalRecoveryIgnoredTransactionReason::RolledBack,
            FileWalRecoveryIgnoredTransactionReason::Incomplete,
        ]
    );
}

fn write_two_committed_transactions(path: &Path) -> Lsn {
    let first_transaction = TransactionId::new(704);
    let second_transaction = TransactionId::new(705);
    let mut wal = FileWal::open(path).unwrap();

    wal.append_tx_begin(first_transaction).unwrap();
    let first_row_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(first_transaction), b"first")
        .unwrap();
    wal.append_tx_commit(first_transaction).unwrap();
    wal.append_tx_begin(second_transaction).unwrap();
    wal.append_payload(
        WalRecordKind::RowUpdate,
        Some(second_transaction),
        b"second",
    )
    .unwrap();
    wal.append_tx_commit(second_transaction).unwrap();
    wal.flush_all().unwrap();

    first_row_lsn
}
