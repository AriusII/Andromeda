use std::sync::Arc;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_time::EngineTimestamp;

use andromeda_transaction::{
    CommitLogManager, IsolationLevel, Lsn, TransactionStatusTable, TxWalAdapterError,
    TxWalAdapterReplayKind, TxWalAdapterReplayRecord, TxWalReplayRecord, WalManager, WalRecordKind,
    append_commit_and_flush, map_tx_wal_replay_records,
};
use andromeda_types::TransactionId;

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

struct NoopWal;

#[async_trait::async_trait]
impl andromeda_transaction_log::InvocationWal for NoopWal {
    async fn append(
        &self,
        _kind: WalRecordKind,
        _transaction_id: Option<TransactionId>,
        _payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        Ok(Lsn::new(1))
    }

    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
        Ok(lsn)
    }
}

struct StaticWal {
    append_result: AndromedaResult<Lsn>,
    flush_result: AndromedaResult<Lsn>,
}

#[async_trait::async_trait]
impl WalManager for StaticWal {
    async fn append_commit(&self, _tx_id: TransactionId) -> AndromedaResult<Lsn> {
        self.append_result.clone()
    }

    async fn flush_through(&self, _lsn: Lsn) -> AndromedaResult<Lsn> {
        self.flush_result.clone()
    }
}

fn storage_error(message: &str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

fn expect_error<T>(result: AndromedaResult<T>) -> AndromedaError {
    match result {
        Ok(_) => panic!("expected WAL adapter error"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn append_commit_and_flush_returns_commit_lsn_after_durable_flush() -> AndromedaResult<()> {
    let wal = Arc::new(StaticWal {
        append_result: Ok(Lsn::new(7)),
        flush_result: Ok(Lsn::new(9)),
    });

    let commit_lsn = append_commit_and_flush(TransactionId::new(50), wal).await?;

    assert_eq!(commit_lsn, Lsn::new(7));
    Ok(())
}

#[tokio::test]
async fn append_commit_and_flush_rejects_short_flush_before_visibility() {
    let wal = Arc::new(StaticWal {
        append_result: Ok(Lsn::new(10)),
        flush_result: Ok(Lsn::new(9)),
    });

    let error = expect_error(append_commit_and_flush(TransactionId::new(51), wal).await);

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(
        error.message(),
        TxWalAdapterError::DurableLsnBehindCommit.message()
    );
}

#[tokio::test]
async fn append_commit_and_flush_maps_append_and_flush_failures_to_typed_storage_errors() {
    let append_error = expect_error(
        append_commit_and_flush(
            TransactionId::new(52),
            Arc::new(StaticWal {
                append_result: Err(storage_error("disk full")),
                flush_result: Ok(Lsn::new(1)),
            }),
        )
        .await,
    );
    assert_eq!(append_error.kind(), AndromedaErrorKind::Storage);
    assert!(append_error.message().contains("WAL append failed"));

    let flush_error = expect_error(
        append_commit_and_flush(
            TransactionId::new(53),
            Arc::new(StaticWal {
                append_result: Ok(Lsn::new(12)),
                flush_result: Err(storage_error("flush failed")),
            }),
        )
        .await,
    );
    assert_eq!(flush_error.kind(), AndromedaErrorKind::Storage);
    assert!(flush_error.message().contains("WAL flush failed"));
}

#[test]
fn maps_begin_commit_and_rollback_boundaries_to_tx_replay_records() -> AndromedaResult<()> {
    let committed = TransactionId::new(10);
    let rolled_back = TransactionId::new(11);

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
    let committed = TransactionId::new(12);
    let rolled_back = TransactionId::new(13);
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
    let tx_id = TransactionId::new(14);

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
    let tx_id = TransactionId::new(20);

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
    let tx_id = TransactionId::new(22);

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
fn begin_only_transaction_maps_incomplete_and_replays_invisible() -> AndromedaResult<()> {
    let tx_id = TransactionId::new(21);
    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(13)),
        TxWalAdapterReplayRecord::other(Lsn::new(14), Some(tx_id)),
    ])?;

    assert_eq!(
        records,
        vec![TxWalReplayRecord::incomplete(tx_id, Lsn::new(14))]
    );

    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(Arc::new(NoopWal), status_table.clone());
    let summary = commit_log.reconstruct_from_tx_wal_replay(records)?;

    assert_eq!(summary.incomplete_transactions, 1);
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_committed(tx_id));
    assert!(!commit_log.is_rolled_back(tx_id));
    Ok(())
}

#[test]
fn rejects_conflicting_terminal_records_for_same_transaction() {
    let tx_id = TransactionId::new(30);

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
        TxWalAdapterReplayRecord::other(Lsn::new(25), Some(TransactionId::new(31))),
    ]));

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        error.message(),
        TxWalAdapterError::ReplayRecordWithoutBegin.message()
    );
}

#[test]
fn rejects_transaction_record_after_terminal_boundary() {
    let tx_id = TransactionId::new(32);

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
    let tx_id = TransactionId::new(33);

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
    let first = TransactionId::new(34);
    let second = TransactionId::new(35);

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
    let first = TransactionId::new(42);
    let second = TransactionId::new(43);

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
    let tx_id = TransactionId::new(36);

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
    let tx_id = TransactionId::new(37);

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
    let tx_id = TransactionId::new(40);

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
