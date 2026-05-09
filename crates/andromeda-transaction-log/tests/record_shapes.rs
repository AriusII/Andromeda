use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_time::EngineTimestamp;
use andromeda_transaction_log::{
    CommitLogEntry, IsolationLevel, Lsn, RollbackLogEntry, TX_COMMIT_PAYLOAD_LEN,
    TX_ROLLBACK_PAYLOAD_LEN, TransactionStatusRebuild, TxWalAdapterError, TxWalAdapterReplayKind,
    TxWalAdapterReplayRecord, TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary,
    WalRecordKind, encode_commit_payload, encode_rollback_payload, map_tx_wal_replay_records,
};
use andromeda_types::TransactionId;

fn tx(id: u64) -> TransactionId {
    TransactionId::new(id)
}

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

fn expect_error<T>(result: AndromedaResult<T>) -> andromeda_error::AndromedaError {
    match result {
        Ok(_) => panic!("expected WAL adapter error"),
        Err(error) => error,
    }
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

#[test]
fn maps_begin_commit_and_rollback_boundaries_to_tx_replay_records() -> AndromedaResult<()> {
    let committed = tx(10);
    let rolled_back = tx(11);

    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(committed, Lsn::new(1)),
        TxWalAdapterReplayRecord::other(Lsn::new(2), Some(committed)),
        TxWalAdapterReplayRecord::commit(committed, Lsn::new(3), ts(100)).with_commit_metadata(
            7,
            IsolationLevel::Serializable,
            0xABCD,
        ),
        TxWalAdapterReplayRecord::begin(rolled_back, Lsn::new(4)),
        TxWalAdapterReplayRecord::rollback(rolled_back, Lsn::new(5), ts(101))
            .with_parameter_hash(0xCAFE),
    ])?;

    assert_eq!(
        records,
        vec![
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(3),
                ts(100),
                7,
                IsolationLevel::Serializable,
            ),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(5), ts(101), 0xCAFE),
        ]
    );
    Ok(())
}

#[test]
fn exact_duplicate_terminal_boundaries_are_idempotent_and_preserve_first_payload()
-> AndromedaResult<()> {
    let committed = tx(12);
    let rolled_back = tx(13);
    let commit = TxWalAdapterReplayRecord::commit(committed, Lsn::new(2), ts(10))
        .with_commit_metadata(3, IsolationLevel::Snapshot, 0xAAAA);
    let rollback = TxWalAdapterReplayRecord::rollback(rolled_back, Lsn::new(5), ts(12))
        .with_parameter_hash(0x1111);

    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(committed, Lsn::new(1)),
        commit,
        commit,
        TxWalAdapterReplayRecord::begin(rolled_back, Lsn::new(4)),
        rollback,
        rollback,
    ])?;

    assert_eq!(
        records,
        vec![
            TxWalReplayRecord::commit(committed, Lsn::new(2), ts(10), 3, IsolationLevel::Snapshot,),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(5), ts(12), 0x1111),
        ]
    );
    Ok(())
}

#[test]
fn divergent_duplicate_terminal_boundaries_are_rejected() {
    let tx_id = tx(14);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(1)),
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(2), ts(10)).with_commit_metadata(
            3,
            IsolationLevel::Snapshot,
            0xAAAA,
        ),
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(3), ts(11)).with_commit_metadata(
            99,
            IsolationLevel::Serializable,
            0xBBBB,
        ),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::ConflictingTerminalRecord.message()
    );
}

#[test]
fn maps_begin_without_terminal_to_incomplete_at_last_transaction_lsn() -> AndromedaResult<()> {
    let tx_id = tx(20);

    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(10)),
        TxWalAdapterReplayRecord::other(Lsn::new(11), Some(tx_id)),
        TxWalAdapterReplayRecord::other(Lsn::new(12), Some(tx_id)),
    ])?;

    assert_eq!(
        records,
        vec![TxWalReplayRecord::incomplete(tx_id, Lsn::new(12))]
    );
    Ok(())
}

#[test]
fn ignores_storage_records_that_do_not_belong_to_a_transaction() -> AndromedaResult<()> {
    let tx_id = tx(22);

    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::other(Lsn::new(1), None),
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(2)),
        TxWalAdapterReplayRecord::other(Lsn::new(3), Some(tx_id)),
        TxWalAdapterReplayRecord::other(Lsn::new(9), None),
    ])?;

    assert_eq!(
        records,
        vec![TxWalReplayRecord::incomplete(tx_id, Lsn::new(3))]
    );
    Ok(())
}

#[test]
fn rejects_conflicting_terminal_records_for_same_transaction() {
    let tx_id = tx(30);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(20)),
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(21), ts(200)),
        TxWalAdapterReplayRecord::rollback(tx_id, Lsn::new(22), ts(201)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn rejects_transaction_record_without_begin_evidence() {
    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::other(Lsn::new(25), Some(tx(31))),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::ReplayRecordWithoutBegin.message()
    );
}

#[test]
fn rejects_transaction_record_after_terminal_boundary() {
    let tx_id = tx(32);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(26)),
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(27), ts(210)),
        TxWalAdapterReplayRecord::other(Lsn::new(28), Some(tx_id)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::RecordAfterTerminal.message()
    );
}

#[test]
fn rejects_lsn_regression_for_one_transaction() {
    let tx_id = tx(33);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(30)),
        TxWalAdapterReplayRecord::other(Lsn::new(29), Some(tx_id)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::ReplayLsnRegression.message()
    );
}

#[test]
fn rejects_global_lsn_regression_across_transactions() {
    let first = tx(34);
    let second = tx(35);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(first, Lsn::new(40)),
        TxWalAdapterReplayRecord::begin(second, Lsn::new(39)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::ReplayLsnRegression.message()
    );
}

#[test]
fn rejects_equal_lsn_for_distinct_replay_records() {
    let first = tx(42);
    let second = tx(43);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(first, Lsn::new(45)),
        TxWalAdapterReplayRecord::begin(second, Lsn::new(45)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::ReplayLsnRegression.message()
    );
}

#[test]
fn terminal_replay_record_carries_durable_lsn_to_commit_log_record() -> AndromedaResult<()> {
    let tx_id = tx(36);

    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(1)),
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(2), ts(20)).with_durable_lsn(Lsn::new(5)),
    ])?;

    assert_eq!(
        records,
        vec![TxWalReplayRecord::commit_with_durable_lsn(
            tx_id,
            Lsn::new(2),
            Lsn::new(5),
            ts(20),
            0,
            IsolationLevel::Snapshot,
        )]
    );
    Ok(())
}

#[test]
fn rejects_terminal_replay_record_beyond_durable_prefix() {
    let tx_id = tx(37);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(1)),
        TxWalAdapterReplayRecord::rollback(tx_id, Lsn::new(4), ts(21))
            .with_durable_lsn(Lsn::new(3)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(
        error.message(),
        TxWalAdapterError::DurableLsnBehindTerminal.message()
    );
}

#[test]
fn rejects_terminal_replay_record_without_begin_evidence() {
    let tx_id = tx(40);

    let error = expect_error(map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(31), ts(300)),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn rejects_boundary_record_without_transaction_id() {
    let error = expect_error(map_tx_wal_replay_records([TxWalAdapterReplayRecord {
        kind: TxWalAdapterReplayKind::Begin,
        lsn: Lsn::new(41),
        durable_lsn: Lsn::new(41),
        tx_id: None,
        timestamp: EngineTimestamp::ZERO,
        row_count_affected: 0,
        isolation_level: IsolationLevel::Snapshot,
        parameter_hash: 0,
    }]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}
