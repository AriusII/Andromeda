use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_transaction_log::Lsn;
use andromeda_types::TransactionId;

use super::*;

struct MockWalManager {
    next_lsn: Mutex<u64>,
    durable_lsn: Mutex<u64>,
}

impl MockWalManager {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            next_lsn: Mutex::new(1),
            durable_lsn: Mutex::new(0),
        })
    }
}

#[async_trait::async_trait]
impl WalManager for MockWalManager {
    async fn append_commit(&self, _tx_id: TransactionId) -> AndromedaResult<Lsn> {
        let mut next = self
            .next_lsn
            .lock()
            .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
        let current = *next;
        *next = current
            .checked_add(1)
            .ok_or_else(|| TxWalAdapterError::InvariantViolated.into_andromeda_error())?;
        Ok(Lsn::new(current))
    }

    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
        let mut durable = self
            .durable_lsn
            .lock()
            .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
        *durable = lsn.get();
        Ok(lsn)
    }
}

struct MockTxWalAdapter {
    committed_txs: Mutex<HashMap<TransactionId, Lsn>>,
}

impl MockTxWalAdapter {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            committed_txs: Mutex::new(HashMap::new()),
        })
    }
}

#[async_trait::async_trait]
impl TxWalAdapterTrait for MockTxWalAdapter {
    async fn record_commit(
        &self,
        tx_id: TransactionId,
        wal_manager: Arc<dyn WalManager>,
    ) -> AndromedaResult<Lsn> {
        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot commit transaction with zero ID",
            ));
        }

        {
            let committed = self
                .committed_txs
                .lock()
                .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
            if let Some(existing_lsn) = committed.get(&tx_id) {
                return Ok(*existing_lsn);
            }
        }

        let lsn = append_commit_and_flush(tx_id, wal_manager).await?;

        let mut committed = self
            .committed_txs
            .lock()
            .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
        committed.insert(tx_id, lsn);
        Ok(lsn)
    }

    async fn record_rollback(&self, _tx_id: TransactionId) -> AndromedaResult<()> {
        Ok(())
    }

    async fn is_durably_committed(&self, tx_id: TransactionId) -> AndromedaResult<bool> {
        let committed = self
            .committed_txs
            .lock()
            .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
        Ok(committed.contains_key(&tx_id))
    }

    async fn get_commit_lsn(&self, tx_id: TransactionId) -> AndromedaResult<Option<Lsn>> {
        let committed = self
            .committed_txs
            .lock()
            .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
        Ok(committed.get(&tx_id).copied())
    }
}

#[tokio::test]
async fn test_record_commit_assigns_lsn() -> AndromedaResult<()> {
    let wal = MockWalManager::new();
    let adapter = MockTxWalAdapter::new();
    let tx_id = TransactionId::new(1);

    let lsn = adapter.record_commit(tx_id, wal).await?;
    assert_eq!(lsn.get(), 1);
    Ok(())
}

#[tokio::test]
async fn test_commit_idempotency() -> AndromedaResult<()> {
    let wal = MockWalManager::new();
    let adapter = MockTxWalAdapter::new();
    let tx_id = TransactionId::new(1);

    let lsn1 = adapter.record_commit(tx_id, wal.clone()).await?;
    let lsn2 = adapter.record_commit(tx_id, wal).await?;

    assert_eq!(lsn1, lsn2);
    Ok(())
}

#[tokio::test]
async fn test_is_durably_committed_after_record_commit() -> AndromedaResult<()> {
    let wal = MockWalManager::new();
    let adapter = MockTxWalAdapter::new();
    let tx_id = TransactionId::new(1);

    adapter.record_commit(tx_id, wal).await?;
    assert!(adapter.is_durably_committed(tx_id).await?);
    Ok(())
}

#[tokio::test]
async fn test_get_commit_lsn_returns_recorded_lsn() -> AndromedaResult<()> {
    let wal = MockWalManager::new();
    let adapter = MockTxWalAdapter::new();
    let tx_id = TransactionId::new(1);

    let recorded_lsn = adapter.record_commit(tx_id, wal).await?;
    let retrieved_lsn = adapter.get_commit_lsn(tx_id).await?;

    assert_eq!(retrieved_lsn, Some(recorded_lsn));
    Ok(())
}

#[tokio::test]
async fn test_rollback_prevents_durability_check() -> AndromedaResult<()> {
    let adapter = MockTxWalAdapter::new();
    let tx_id = TransactionId::new(1);

    adapter.record_rollback(tx_id).await?;
    assert!(!adapter.is_durably_committed(tx_id).await?);
    Ok(())
}

#[tokio::test]
async fn test_reject_zero_transaction_id() {
    let wal = MockWalManager::new();
    let adapter = MockTxWalAdapter::new();

    let result = adapter.record_commit(TransactionId::new(0), wal).await;
    assert!(matches!(
        result,
        Err(error) if error.kind() == AndromedaErrorKind::Transaction
    ));
}

#[test]
fn test_wal_adapter_error_to_andromeda_error() {
    let err = TxWalAdapterError::WalFlushFailed;
    let andromeda_err = err.into_andromeda_error();
    assert_eq!(andromeda_err.kind(), AndromedaErrorKind::Storage);
}
