use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_time::EngineTimestamp;
use andromeda_transaction_log::{
    CommitLogEntry, IsolationLevel, Lsn, RollbackLogEntry, TX_COMMIT_PAYLOAD_LEN,
    TX_ROLLBACK_PAYLOAD_LEN, TransactionStatusRebuild, TxWalReplayAction, TxWalReplayRecord,
    TxWalReplaySummary, WalRecordKind, encode_commit_payload, encode_rollback_payload,
};
use andromeda_types::TransactionId;

fn tx(id: u64) -> TransactionId {
    TransactionId::new(id)
}

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

#[test]
fn commit_log_entry_shape_requires_durable_commit_evidence() -> AndromedaResult<()> {
    let entry = CommitLogEntry::from_durable_wal(
        tx(10),
        Lsn::new(20),
        Lsn::new(25),
        ts(100),
        3,
        IsolationLevel::Serializable,
    )?;

    assert_eq!(entry.tx_id, tx(10));
    assert_eq!(entry.commit_lsn, Lsn::new(20));
    assert_eq!(entry.durable_lsn, Lsn::new(25));
    assert_eq!(entry.timestamp, ts(100));
    assert_eq!(entry.row_count_affected, 3);
    assert_eq!(entry.isolation_level, IsolationLevel::Serializable);
    assert!(entry.is_before(Lsn::new(21)));
    assert!(!entry.is_before(Lsn::new(20)));

    let zero_tx_error = CommitLogEntry::from_durable_wal(
        tx(0),
        Lsn::new(20),
        Lsn::new(20),
        ts(100),
        0,
        IsolationLevel::Snapshot,
    )
    .expect_err("commit entry must reject zero transaction id");
    assert_eq!(zero_tx_error.kind(), AndromedaErrorKind::Transaction);

    let zero_lsn_error = CommitLogEntry::from_durable_wal(
        tx(11),
        Lsn::new(0),
        Lsn::new(0),
        ts(100),
        0,
        IsolationLevel::Snapshot,
    )
    .expect_err("commit entry must reject zero commit LSN");
    assert_eq!(zero_lsn_error.kind(), AndromedaErrorKind::Transaction);

    let short_flush_error = CommitLogEntry::from_durable_wal(
        tx(11),
        Lsn::new(20),
        Lsn::new(19),
        ts(100),
        0,
        IsolationLevel::Snapshot,
    )
    .expect_err("commit entry must reject durable LSN before commit LSN");
    assert_eq!(short_flush_error.kind(), AndromedaErrorKind::Storage);

    Ok(())
}

#[test]
fn rollback_log_entry_shape_requires_durable_rollback_evidence() -> AndromedaResult<()> {
    let entry = RollbackLogEntry::from_durable_wal(tx(12), Lsn::new(30), Lsn::new(31), ts(200), 7)?;

    assert_eq!(entry.tx_id, tx(12));
    assert_eq!(entry.rollback_lsn, Lsn::new(30));
    assert_eq!(entry.durable_lsn, Lsn::new(31));
    assert_eq!(entry.timestamp, ts(200));
    assert_eq!(entry.parameter_hash, 7);

    let zero_tx_error =
        RollbackLogEntry::from_durable_wal(tx(0), Lsn::new(30), Lsn::new(30), ts(200), 0)
            .expect_err("rollback entry must reject zero transaction id");
    assert_eq!(zero_tx_error.kind(), AndromedaErrorKind::Transaction);

    let zero_lsn_error =
        RollbackLogEntry::from_durable_wal(tx(13), Lsn::new(0), Lsn::new(0), ts(200), 0)
            .expect_err("rollback entry must reject zero rollback LSN");
    assert_eq!(zero_lsn_error.kind(), AndromedaErrorKind::Transaction);

    let short_flush_error =
        RollbackLogEntry::from_durable_wal(tx(13), Lsn::new(30), Lsn::new(29), ts(200), 0)
            .expect_err("rollback entry must reject durable LSN before rollback LSN");
    assert_eq!(short_flush_error.kind(), AndromedaErrorKind::Storage);

    Ok(())
}

#[test]
fn tx_wal_replay_record_classifies_terminal_and_incomplete_shapes() {
    let commit = TxWalReplayRecord::commit_with_durable_lsn(
        tx(20),
        Lsn::new(40),
        Lsn::new(41),
        ts(300),
        5,
        IsolationLevel::Snapshot,
    );
    assert_eq!(commit.tx_id(), tx(20));
    assert_eq!(commit.replay_lsn(), Lsn::new(40));
    let TxWalReplayRecord::Commit(commit_entry) = commit else {
        panic!("commit factory must create a commit replay record");
    };
    assert_eq!(commit_entry.tx_id, tx(20));
    assert_eq!(commit_entry.commit_lsn, Lsn::new(40));
    assert_eq!(commit_entry.durable_lsn, Lsn::new(41));
    assert_eq!(commit_entry.timestamp, ts(300));
    assert_eq!(commit_entry.row_count_affected, 5);
    assert_eq!(commit_entry.isolation_level, IsolationLevel::Snapshot);

    let rollback = TxWalReplayRecord::rollback_with_durable_lsn(
        tx(21),
        Lsn::new(50),
        Lsn::new(51),
        ts(301),
        9,
    );
    assert_eq!(rollback.tx_id(), tx(21));
    assert_eq!(rollback.replay_lsn(), Lsn::new(50));
    let TxWalReplayRecord::Rollback(rollback_entry) = rollback else {
        panic!("rollback factory must create a rollback replay record");
    };
    assert_eq!(rollback_entry.tx_id, tx(21));
    assert_eq!(rollback_entry.rollback_lsn, Lsn::new(50));
    assert_eq!(rollback_entry.durable_lsn, Lsn::new(51));
    assert_eq!(rollback_entry.timestamp, ts(301));
    assert_eq!(rollback_entry.parameter_hash, 9);

    let incomplete = TxWalReplayRecord::incomplete(tx(22), Lsn::new(60));
    assert_eq!(incomplete.tx_id(), tx(22));
    assert_eq!(incomplete.replay_lsn(), Lsn::new(60));
    let TxWalReplayRecord::Incomplete { tx_id, last_lsn } = incomplete else {
        panic!("incomplete factory must create an incomplete replay record");
    };
    assert_eq!(tx_id, tx(22));
    assert_eq!(last_lsn, Lsn::new(60));
}

#[test]
fn tx_wal_replay_summary_counts_classification_actions() {
    let mut summary = TxWalReplaySummary::default();

    summary.record(TxWalReplayAction::CommitRestored);
    summary.record(TxWalReplayAction::RollbackRestored);
    summary.record(TxWalReplayAction::DuplicateCommit);
    summary.record(TxWalReplayAction::DuplicateRollback);
    summary.record(TxWalReplayAction::IncompleteIgnored);
    summary.record(TxWalReplayAction::IncompleteIgnored);

    assert_eq!(
        summary,
        TxWalReplaySummary {
            commits_restored: 1,
            rollbacks_restored: 1,
            duplicate_commits: 1,
            duplicate_rollbacks: 1,
            incomplete_transactions: 2,
        }
    );
}

#[test]
fn terminal_wal_payloads_use_explicit_record_shapes() {
    let commit_payload = encode_commit_payload(
        IsolationLevel::Serializable,
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
    );
    assert_eq!(commit_payload.len(), TX_COMMIT_PAYLOAD_LEN);
    assert_eq!(commit_payload[0], 2);
    assert_eq!(
        &commit_payload[1..9],
        &0x0102_0304_0506_0708_u64.to_le_bytes()
    );
    assert_eq!(
        &commit_payload[9..17],
        &0x1112_1314_1516_1718_u64.to_le_bytes()
    );
    assert_eq!(&commit_payload[17..25], &0_u64.to_le_bytes());

    let rollback_payload = encode_rollback_payload(0x2122_2324_2526_2728);
    assert_eq!(rollback_payload.len(), TX_ROLLBACK_PAYLOAD_LEN);
    assert_eq!(
        &rollback_payload[0..8],
        &0x2122_2324_2526_2728_u64.to_le_bytes()
    );
    assert_eq!(&rollback_payload[8..16], &0_u64.to_le_bytes());

    assert_eq!(WalRecordKind::TxCommit, WalRecordKind::TxCommit);
    assert_ne!(WalRecordKind::TxCommit, WalRecordKind::TxRollback);
}

#[test]
fn status_rebuild_summary_is_a_shape_only_counter() {
    let summary = TransactionStatusRebuild {
        committed_restored: 2,
        rolled_back_restored: 1,
        already_present: 3,
    };

    assert_eq!(summary.committed_restored, 2);
    assert_eq!(summary.rolled_back_restored, 1);
    assert_eq!(summary.already_present, 3);
}
