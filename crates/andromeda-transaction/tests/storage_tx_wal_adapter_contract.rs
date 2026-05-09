use std::sync::Arc;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_time::EngineTimestamp;
use andromeda_transaction::{
    IsolationLevel, Lsn, TxWalAdapterError, TxWalAdapterReplayRecord, TxWalReplayRecord,
    WalManager, append_commit_and_flush, map_tx_wal_replay_records,
};
use andromeda_types::TransactionId;

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

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
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
fn transaction_facade_preserves_replay_mapper_compatibility() -> AndromedaResult<()> {
    let tx_id = TransactionId::new(10);

    let records = map_tx_wal_replay_records([
        TxWalAdapterReplayRecord::begin(tx_id, Lsn::new(1)),
        TxWalAdapterReplayRecord::commit(tx_id, Lsn::new(2), ts(100)).with_commit_metadata(
            7,
            IsolationLevel::Serializable,
            0xABCD,
        ),
    ])?;

    assert_eq!(
        records,
        vec![TxWalReplayRecord::commit(
            tx_id,
            Lsn::new(2),
            ts(100),
            7,
            IsolationLevel::Serializable,
        )]
    );
    Ok(())
}
