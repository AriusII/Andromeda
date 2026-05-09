use crate::support::{TempWalPath, manifest};
use andromeda_recovery::StartupMode;
use andromeda_recovery::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason,
    report_file_wal_recovery_v0,
};
use andromeda_types::TransactionId;
use andromeda_wal::FileWal;
use andromeda_wal::Lsn;
use andromeda_wal::WalRecordKind;

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
