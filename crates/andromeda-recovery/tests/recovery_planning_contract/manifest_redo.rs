use crate::support::recovery_manifest;
use andromeda_error::AndromedaErrorKind;
use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{RecoveryPlan, RedoRecordDecision, StartupMode};
use andromeda_types::TransactionId;
use andromeda_wal::{DurableTransactionState, InMemoryWal, Lsn, WalRecord, WalRecordKind};

#[test]
fn manifest_keeps_snapshot_plus_wal_anchor() {
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(100),
        required_wal_start_lsn: Lsn::new(101),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };

    assert!(manifest.validate().is_ok());
    assert_eq!(
        RecoveryPlan::from_manifest(&manifest, StartupMode::SafeStart)
            .unwrap()
            .redo_from_lsn,
        Lsn::new(101)
    );
}

#[test]
fn redo_plan_replays_only_manifest_range_and_complete_transactions() {
    let incomplete_tx = TransactionId::new(21);
    let committed_tx = TransactionId::new(22);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(incomplete_tx).unwrap();
    let incomplete_row_lsn = wal
        .append_payload(WalRecordKind::RowInsert, Some(incomplete_tx), b"incomplete")
        .unwrap();
    wal.append_tx_begin(committed_tx).unwrap();
    let committed_row_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(committed_tx), b"complete")
        .unwrap();
    wal.append_tx_commit(committed_tx).unwrap();
    wal.flush_all().unwrap();

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: incomplete_row_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let durable_records = wal.replay_durable();
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .unwrap();

    assert!(plan.has_incomplete_transactions());
    assert_eq!(
        plan.incomplete_transactions[0].transaction_id,
        incomplete_tx
    );
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![committed_row_lsn]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == incomplete_row_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn redo_plan_skips_transaction_when_commit_is_not_durable() {
    let transaction_id = TransactionId::new(23);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(transaction_id).unwrap();
    let row_lsn = wal
        .append_payload(
            WalRecordKind::RowDelete,
            Some(transaction_id),
            b"not-committed",
        )
        .unwrap();
    let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
    wal.flush_through(row_lsn).unwrap();

    let manifest = recovery_manifest(Lsn::new(1), Lsn::new(1));
    let durable_records = wal.replay_durable();
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .unwrap();

    assert_eq!(commit_lsn, Lsn::new(3));
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(plan.incomplete_transactions.len(), 1);
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == row_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn redo_plan_skips_transaction_with_commit_but_missing_begin_evidence() {
    let transaction_id = TransactionId::new(24);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(1),
            None,
            Some(transaction_id),
            b"missing-begin".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = recovery_manifest(Lsn::new(1), Lsn::new(1));

    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records).unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
    assert_eq!(
        plan.transaction_evidence[0].state,
        DurableTransactionState::Incomplete
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(1))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn redo_plan_does_not_treat_non_durable_ram_records_as_truth() {
    let tx = TransactionId::new(60);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).unwrap();
    let _ram_only_row = wal
        .append_payload(WalRecordKind::RowInsert, Some(tx), b"ram-only")
        .unwrap();
    wal.append_tx_commit(tx).unwrap();

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let durable_records = wal.replay_durable();
    assert!(
        durable_records.is_empty(),
        "non-flushed appends must not appear in the durable replay slice"
    );

    let result =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records);
    assert_eq!(
        result.unwrap_err().kind(),
        AndromedaErrorKind::Storage,
        "missing required WAL start LSN must surface as a storage error",
    );
}
