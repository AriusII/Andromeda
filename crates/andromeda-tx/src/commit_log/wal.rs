use andromeda_core::{AndromedaResult, TransactionId};

use crate::Lsn;

use super::entry::{IsolationLevel, WalRecordKind};

/// WAL boundary used by the transaction commit log.
#[async_trait::async_trait]
pub trait InvocationWal: Send + Sync {
    async fn append(
        &self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn>;

    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn>;
}

pub(super) fn encode_commit_payload(
    isolation_level: IsolationLevel,
    affected_rows: u64,
    parameter_hash: u64,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(25);
    payload.push(match isolation_level {
        IsolationLevel::Snapshot => 1,
        IsolationLevel::Serializable => 2,
    });
    payload.extend_from_slice(&affected_rows.to_le_bytes());
    payload.extend_from_slice(&parameter_hash.to_le_bytes());
    payload.extend_from_slice(&0_u64.to_le_bytes());
    payload
}

pub(super) fn encode_rollback_payload(parameter_hash: u64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(16);
    payload.extend_from_slice(&parameter_hash.to_le_bytes());
    payload.extend_from_slice(&0_u64.to_le_bytes());
    payload
}
