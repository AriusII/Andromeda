use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;
use std::future::Future;
use std::pin::Pin;

use crate::Lsn;
use crate::entry::{IsolationLevel, WalRecordKind};

pub type InvocationWalFuture<'a, T> = Pin<Box<dyn Future<Output = AndromedaResult<T>> + Send + 'a>>;

/// WAL boundary used by the transaction commit log.
pub trait InvocationWal: Send + Sync {
    fn append<'life0, 'life1, 'async_trait>(
        &'life0 self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &'life1 [u8],
    ) -> InvocationWalFuture<'async_trait, Lsn>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait;

    fn flush_through<'life0, 'async_trait>(
        &'life0 self,
        lsn: Lsn,
    ) -> InvocationWalFuture<'async_trait, Lsn>
    where
        'life0: 'async_trait,
        Self: 'async_trait;
}

pub const TX_COMMIT_PAYLOAD_LEN: usize = 25;
pub const TX_ROLLBACK_PAYLOAD_LEN: usize = 16;

pub fn encode_commit_payload(
    isolation_level: IsolationLevel,
    affected_rows: u64,
    parameter_hash: u64,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(TX_COMMIT_PAYLOAD_LEN);
    payload.push(match isolation_level {
        IsolationLevel::Snapshot => 1,
        IsolationLevel::Serializable => 2,
    });
    payload.extend_from_slice(&affected_rows.to_le_bytes());
    payload.extend_from_slice(&parameter_hash.to_le_bytes());
    payload.extend_from_slice(&0_u64.to_le_bytes());
    payload
}

pub fn encode_rollback_payload(parameter_hash: u64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(TX_ROLLBACK_PAYLOAD_LEN);
    payload.extend_from_slice(&parameter_hash.to_le_bytes());
    payload.extend_from_slice(&0_u64.to_le_bytes());
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_payload_uses_explicit_little_endian_layout() {
        let payload = encode_commit_payload(
            IsolationLevel::Serializable,
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
        );

        assert_eq!(payload.len(), TX_COMMIT_PAYLOAD_LEN);
        assert_eq!(payload[0], 2);
        assert_eq!(&payload[1..9], &0x0102_0304_0506_0708_u64.to_le_bytes());
        assert_eq!(&payload[9..17], &0x1112_1314_1516_1718_u64.to_le_bytes());
        assert_eq!(&payload[17..25], &0_u64.to_le_bytes());
    }

    #[test]
    fn rollback_payload_uses_explicit_little_endian_layout() {
        let payload = encode_rollback_payload(0x2122_2324_2526_2728);

        assert_eq!(payload.len(), TX_ROLLBACK_PAYLOAD_LEN);
        assert_eq!(&payload[0..8], &0x2122_2324_2526_2728_u64.to_le_bytes());
        assert_eq!(&payload[8..16], &0_u64.to_le_bytes());
    }
}
