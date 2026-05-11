use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason,
    FileWalRecoveryReplayRecord, FileWalRecoverySkippedNonRedoRecord, ObservedBoundary,
    RedoRecordDecision, StartupMode, plan_file_wal_startup_recovery_v0, recover_from_file_wal,
    report_file_wal_recovery_v0,
};
use andromeda_types::TransactionId;
use andromeda_wal::{FILE_WAL_HEADER_LEN, FileWal, Lsn, WalRecordKind};
use std::fs::{metadata, remove_file};
use std::path::PathBuf;

fn test_wal_path(test_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "andromeda-recovery-{test_name}-{}.wal",
        std::process::id()
    ))
}

fn recovery_manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    }
}

#[test]
fn recovery_owner_plans_file_wal_startup_from_durable_evidence() {
    let path = test_wal_path("owner-startup-durable-evidence");
    remove_file(&path).ok();
    let committed_tx = TransactionId::new(301);
    let incomplete_tx = TransactionId::new(302);
    let rolled_back_tx = TransactionId::new(303);

    let (
        committed_begin_lsn,
        committed_insert_lsn,
        committed_update_lsn,
        committed_commit_lsn,
        incomplete_lsn,
        rolled_back_lsn,
        durable_lsn,
    ) = {
        let mut wal = FileWal::open(&path).unwrap();
        let committed_begin_lsn = wal.append_tx_begin(committed_tx).unwrap();
        let committed_insert_lsn = wal
            .append_payload(
                WalRecordKind::RowInsert,
                Some(committed_tx),
                b"client-ack-is-not-recovery-proof",
            )
            .unwrap();
        let committed_update_lsn = wal
            .append_payload(
                WalRecordKind::RowUpdate,
                Some(committed_tx),
                b"result-stream-is-not-recovery-proof",
            )
            .unwrap();
        let committed_commit_lsn = wal.append_tx_commit(committed_tx).unwrap();
        wal.append_tx_begin(incomplete_tx).unwrap();
        let incomplete_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(incomplete_tx), b"incomplete")
            .unwrap();
        wal.append_tx_begin(rolled_back_tx).unwrap();
        let rolled_back_lsn = wal
            .append_payload(
                WalRecordKind::RowDelete,
                Some(rolled_back_tx),
                b"rolled-back",
            )
            .unwrap();
        wal.append_tx_rollback(rolled_back_tx).unwrap();
        let durable_lsn = wal.flush_all().unwrap();

        (
            committed_begin_lsn,
            committed_insert_lsn,
            committed_update_lsn,
            committed_commit_lsn,
            incomplete_lsn,
            rolled_back_lsn,
            durable_lsn,
        )
    };

    assert_eq!(committed_begin_lsn, Lsn::new(1));
    assert_eq!(committed_insert_lsn, Lsn::new(2));
    assert_eq!(committed_update_lsn, Lsn::new(3));
    assert_eq!(committed_commit_lsn, Lsn::new(4));
    assert_eq!(incomplete_lsn, Lsn::new(6));
    assert_eq!(rolled_back_lsn, Lsn::new(8));
    assert_eq!(durable_lsn, Lsn::new(9));

    let startup = plan_file_wal_startup_recovery_v0(
        &recovery_manifest(),
        StartupMode::SafeStart,
        &path,
        false,
    )
    .unwrap();
    let redo = startup.redo_plan.as_ref().unwrap();

    assert!(startup.decision.is_accepted());
    assert_eq!(
        startup.evidence.observed_boundary(),
        ObservedBoundary::Clean
    );
    assert_eq!(startup.startup_mode, StartupMode::SafeStart);
    assert_eq!(startup.disk_scan.durable_lsn, durable_lsn);
    assert_eq!(startup.evidence.last_durable_lsn, durable_lsn);
    assert_eq!(redo.durable_lsn, durable_lsn);
    assert_eq!(
        redo.committed_replay_lsns().collect::<Vec<_>>(),
        vec![committed_insert_lsn, committed_update_lsn]
    );
    assert_eq!(
        redo.replay_lsns().collect::<Vec<_>>(),
        vec![committed_insert_lsn, committed_update_lsn]
    );
    assert_eq!(
        redo.records
            .iter()
            .find(|record| record.lsn == committed_begin_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipNonRedoRecord
    );
    assert_eq!(
        redo.records
            .iter()
            .find(|record| record.lsn == committed_commit_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipNonRedoRecord
    );
    assert_eq!(
        redo.records
            .iter()
            .find(|record| record.lsn == incomplete_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
    assert_eq!(
        redo.records
            .iter()
            .find(|record| record.lsn == rolled_back_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        startup.transaction_manager_allocator_floor(),
        rolled_back_tx.get()
    );

    let report =
        report_file_wal_recovery_v0(&recovery_manifest(), StartupMode::SafeStart, &path).unwrap();
    assert_eq!(report.startup_mode, StartupMode::SafeStart);
    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert_eq!(report.durable_lsn, durable_lsn);
    assert_eq!(report.header.durable_lsn, durable_lsn);
    assert_eq!(
        report.replay_records,
        vec![
            FileWalRecoveryReplayRecord {
                lsn: committed_insert_lsn,
                kind: WalRecordKind::RowInsert,
                transaction_id: Some(committed_tx),
            },
            FileWalRecoveryReplayRecord {
                lsn: committed_update_lsn,
                kind: WalRecordKind::RowUpdate,
                transaction_id: Some(committed_tx),
            },
        ]
    );
    assert_eq!(
        report.replay_lsns().collect::<Vec<_>>(),
        vec![committed_insert_lsn, committed_update_lsn]
    );
    assert_eq!(
        report.skipped_non_redo_records,
        vec![
            FileWalRecoverySkippedNonRedoRecord {
                lsn: committed_begin_lsn,
                kind: WalRecordKind::TxBegin,
                transaction_id: Some(committed_tx),
            },
            FileWalRecoverySkippedNonRedoRecord {
                lsn: committed_commit_lsn,
                kind: WalRecordKind::TxCommit,
                transaction_id: Some(committed_tx),
            },
            FileWalRecoverySkippedNonRedoRecord {
                lsn: Lsn::new(5),
                kind: WalRecordKind::TxBegin,
                transaction_id: Some(incomplete_tx),
            },
            FileWalRecoverySkippedNonRedoRecord {
                lsn: Lsn::new(7),
                kind: WalRecordKind::TxBegin,
                transaction_id: Some(rolled_back_tx),
            },
            FileWalRecoverySkippedNonRedoRecord {
                lsn: Lsn::new(9),
                kind: WalRecordKind::TxRollback,
                transaction_id: Some(rolled_back_tx),
            },
        ]
    );
    assert_eq!(
        report.skipped_non_redo_lsns().collect::<Vec<_>>(),
        vec![
            Lsn::new(1),
            Lsn::new(4),
            Lsn::new(5),
            Lsn::new(7),
            Lsn::new(9)
        ]
    );
    assert_eq!(report.ignored_transactions.len(), 2);
    assert_eq!(report.ignored_transactions[0].transaction_id, incomplete_tx);
    assert_eq!(
        report.ignored_transactions[0].reason,
        FileWalRecoveryIgnoredTransactionReason::Incomplete
    );
    assert_eq!(
        report.ignored_transactions[1].transaction_id,
        rolled_back_tx
    );
    assert_eq!(
        report.ignored_transactions[1].reason,
        FileWalRecoveryIgnoredTransactionReason::RolledBack
    );
    assert_eq!(report.ignored_record_count, 2);

    let plan = recover_from_file_wal(&recovery_manifest(), StartupMode::SafeStart, &path).unwrap();
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![committed_insert_lsn, committed_update_lsn]
    );
    assert_eq!(
        metadata(&path).unwrap().len(),
        report.physical_wal_bytes + FILE_WAL_HEADER_LEN as u64
    );

    remove_file(&path).ok();
}
