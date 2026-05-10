use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_time::EngineTimestamp;
use andromeda_transaction_log::{
    InvocationWal as TransactionInvocationWal, IsolationLevel, Lsn as TxLsn, TX_COMMIT_PAYLOAD_LEN,
    TX_ROLLBACK_PAYLOAD_LEN, TxWalAdapterReplayRecord, TxWalReplayRecord,
    WalRecordKind as TxWalRecordKind, map_tx_wal_replay_records,
};
use andromeda_types::TransactionId;
use andromeda_wal::{FileWal, InMemoryWal, InvocationWal, Lsn, WalRecord, WalRecordKind};
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::{
    future::Future,
    pin::Pin,
    sync::{Mutex, MutexGuard},
};

pub struct CommitLogInvocationWal<W> {
    wal: Mutex<W>,
}

impl<W> CommitLogInvocationWal<W> {
    pub fn new(wal: W) -> Self {
        Self {
            wal: Mutex::new(wal),
        }
    }

    pub fn into_inner(self) -> AndromedaResult<W> {
        self.wal.into_inner().map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Internal,
                "commit log WAL bridge mutex was poisoned",
            )
        })
    }

    pub fn append_tx_begin(&self, transaction_id: TransactionId) -> AndromedaResult<TxLsn>
    where
        W: InvocationWal,
    {
        let mut wal = self.lock_wal()?;
        let lsn = wal.append(WalRecordKind::TxBegin, Some(transaction_id), &[])?;
        Ok(tx_lsn_from_storage_lsn(lsn))
    }

    pub fn flush_through_tx_lsn(&self, lsn: TxLsn) -> AndromedaResult<TxLsn>
    where
        W: InvocationWal,
    {
        let mut wal = self.lock_wal()?;
        let durable_lsn = wal.flush_through(storage_lsn_from_tx_lsn(lsn))?;
        Ok(tx_lsn_from_storage_lsn(durable_lsn))
    }

    fn lock_wal(&self) -> AndromedaResult<MutexGuard<'_, W>> {
        self.wal.lock().map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Internal,
                "commit log WAL bridge mutex was poisoned",
            )
        })
    }
}

impl<W> TransactionInvocationWal for CommitLogInvocationWal<W>
where
    W: InvocationWal + Send + 'static,
{
    fn append<'life0, 'life1, 'async_trait>(
        &'life0 self,
        kind: TxWalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &'life1 [u8],
    ) -> Pin<Box<dyn Future<Output = AndromedaResult<TxLsn>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let payload = payload.to_vec();
        Box::pin(async move {
            let storage_kind = match kind {
                TxWalRecordKind::TxCommit => WalRecordKind::TxCommit,
                TxWalRecordKind::TxRollback => WalRecordKind::TxRollback,
            };
            let mut wal = self.lock_wal()?;
            let lsn = wal.append(storage_kind, transaction_id, &payload)?;
            Ok(tx_lsn_from_storage_lsn(lsn))
        })
    }

    fn flush_through<'life0, 'async_trait>(
        &'life0 self,
        lsn: TxLsn,
    ) -> Pin<Box<dyn Future<Output = AndromedaResult<TxLsn>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move { self.flush_through_tx_lsn(lsn) })
    }
}

/// Physical WAL records proven to be within a durable WAL prefix.
#[derive(Debug, Clone)]
pub struct DurableTransactionWalPrefix<R> {
    records: Vec<R>,
    durable_lsn: Lsn,
}

impl<R> DurableTransactionWalPrefix<R>
where
    R: Borrow<WalRecord>,
{
    pub fn new(records: impl IntoIterator<Item = R>, durable_lsn: Lsn) -> AndromedaResult<Self> {
        let records = records.into_iter().collect::<Vec<_>>();
        for record in &records {
            let record = record.borrow();
            record.validate()?;
            if record.header.lsn > durable_lsn {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "transaction WAL replay evidence contains a non-durable source record",
                ));
            }
        }

        Ok(Self {
            records,
            durable_lsn,
        })
    }

    pub const fn durable_lsn(&self) -> Lsn {
        self.durable_lsn
    }
}

impl<'a> DurableTransactionWalPrefix<&'a WalRecord> {
    pub fn from_in_memory_wal(wal: &'a InMemoryWal) -> AndromedaResult<Self> {
        Self::new(wal.durable_records(), wal.durable_lsn())
    }

    pub fn from_file_wal(wal: &'a FileWal) -> AndromedaResult<Self> {
        Self::new(wal.durable_records(), wal.durable_lsn())
    }
}

/// Transaction replay records projected from durable physical WAL evidence.
///
/// This bridge converts WAL records into the transaction crate's storage-agnostic
/// replay adapter without introducing an `andromeda-transaction -> andromeda-wal`
/// dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionReplayFromWalEvidence {
    pub replay_records: Vec<TxWalReplayRecord>,
    pub evidence: TxReplayBridgeEvidence,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TxReplayBridgeEvidence {
    pub source_records: usize,
    pub adapter_records: usize,
    pub commits: usize,
    pub rollbacks: usize,
    pub incomplete_transactions: usize,
}

pub fn map_durable_wal_prefix_to_tx_replay<R>(
    durable_prefix: DurableTransactionWalPrefix<R>,
) -> AndromedaResult<TransactionReplayFromWalEvidence>
where
    R: Borrow<WalRecord>,
{
    map_checked_wal_evidence_to_tx_replay(durable_prefix.records)
}

fn map_checked_wal_evidence_to_tx_replay<R>(
    records: impl IntoIterator<Item = R>,
) -> AndromedaResult<TransactionReplayFromWalEvidence>
where
    R: Borrow<WalRecord>,
{
    let mut evidence = TxReplayBridgeEvidence::default();
    let mut adapter_records = Vec::new();
    let mut seen_lsns = BTreeSet::new();

    for record in records {
        let record = record.borrow();
        record.validate()?;
        if !seen_lsns.insert(record.header.lsn) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "transaction WAL replay evidence contains duplicate source LSN",
            ));
        }
        evidence.source_records += 1;

        let tx_lsn = tx_lsn_from_storage_lsn(record.header.lsn);
        match record.header.kind {
            WalRecordKind::TxBegin => {
                adapter_records.push(TxWalAdapterReplayRecord::begin(
                    required_tx_id(record)?,
                    tx_lsn,
                ));
                evidence.adapter_records += 1;
            },
            WalRecordKind::TxCommit => {
                let metadata = decode_commit_payload(record.payload())?;
                adapter_records.push(
                    TxWalAdapterReplayRecord::commit(
                        required_tx_id(record)?,
                        tx_lsn,
                        EngineTimestamp::ZERO,
                    )
                    .with_commit_metadata(
                        metadata.row_count_affected,
                        metadata.isolation_level,
                        metadata.parameter_hash,
                    ),
                );
                evidence.adapter_records += 1;
            },
            WalRecordKind::TxRollback => {
                let metadata = decode_rollback_payload(record.payload())?;
                adapter_records.push(
                    TxWalAdapterReplayRecord::rollback(
                        required_tx_id(record)?,
                        tx_lsn,
                        EngineTimestamp::ZERO,
                    )
                    .with_parameter_hash(metadata.parameter_hash),
                );
                evidence.adapter_records += 1;
            },
            _ => {
                if record.header.transaction_id.is_some() {
                    adapter_records.push(TxWalAdapterReplayRecord::other(
                        tx_lsn,
                        record.header.transaction_id,
                    ));
                    evidence.adapter_records += 1;
                }
            },
        }
    }

    adapter_records.sort_by_key(|record| record.lsn);
    let replay_records = map_tx_wal_replay_records(adapter_records)?;
    for replay_record in &replay_records {
        match replay_record {
            TxWalReplayRecord::Commit(_) => evidence.commits += 1,
            TxWalReplayRecord::Rollback(_) => evidence.rollbacks += 1,
            TxWalReplayRecord::Incomplete { .. } => {
                evidence.incomplete_transactions += 1;
            },
        }
    }

    Ok(TransactionReplayFromWalEvidence {
        replay_records,
        evidence,
    })
}

const fn tx_lsn_from_storage_lsn(lsn: Lsn) -> TxLsn {
    TxLsn::new(lsn.get())
}

const fn storage_lsn_from_tx_lsn(lsn: TxLsn) -> Lsn {
    Lsn::new(lsn.get())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommitPayloadMetadata {
    isolation_level: IsolationLevel,
    row_count_affected: u64,
    parameter_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RollbackPayloadMetadata {
    parameter_hash: u64,
}

fn required_tx_id(record: &WalRecord) -> AndromedaResult<TransactionId> {
    let transaction_id = record.header.transaction_id.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "transaction WAL replay evidence is missing transaction id",
        )
    })?;

    if transaction_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "transaction WAL replay evidence contains zero transaction id",
        ));
    }

    Ok(transaction_id)
}

fn decode_commit_payload(payload: &[u8]) -> AndromedaResult<CommitPayloadMetadata> {
    if payload.len() != TX_COMMIT_PAYLOAD_LEN {
        return Err(tx_bridge_error(format!(
            "transaction commit WAL payload has unsupported length: expected {TX_COMMIT_PAYLOAD_LEN} bytes, got {} bytes",
            payload.len()
        )));
    }

    let isolation_level = match payload[0] {
        1 => IsolationLevel::Snapshot,
        2 => IsolationLevel::Serializable,
        _ => {
            return Err(tx_bridge_error(format!(
                "transaction commit WAL payload has unknown isolation level code {}",
                payload[0]
            )));
        },
    };

    Ok(CommitPayloadMetadata {
        isolation_level,
        row_count_affected: read_u64_le(payload, 1)?,
        parameter_hash: read_u64_le(payload, 9)?,
    })
}

fn decode_rollback_payload(payload: &[u8]) -> AndromedaResult<RollbackPayloadMetadata> {
    if payload.len() != TX_ROLLBACK_PAYLOAD_LEN {
        return Err(tx_bridge_error(format!(
            "transaction rollback WAL payload has unsupported length: expected {TX_ROLLBACK_PAYLOAD_LEN} bytes, got {} bytes",
            payload.len()
        )));
    }

    Ok(RollbackPayloadMetadata {
        parameter_hash: read_u64_le(payload, 0)?,
    })
}

fn read_u64_le(payload: &[u8], offset: usize) -> AndromedaResult<u64> {
    let bytes = payload.get(offset..offset + 8).ok_or_else(|| {
        tx_bridge_error(format!(
            "transaction WAL payload is truncated at u64 offset {offset}"
        ))
    })?;
    let mut buffer = [0_u8; 8];
    buffer.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(buffer))
}

fn tx_bridge_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transaction, message)
}
