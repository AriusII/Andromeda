use andromeda_core::{AndromedaResult, TransactionId};

use crate::Lsn;

use super::{storage_error, transaction_error};

pub(super) const U64_FIELD_BYTES: usize = 8;
pub(super) const TX_ID_OFFSET: usize = 0;
pub(super) const COMMIT_LSN_OFFSET: usize = 8;
pub(super) const VISIBLE_TIMESTAMP_OFFSET: usize = 16;
pub(super) const DURABILITY_FLAG_OFFSET: usize = 24;
pub(super) const COMMIT_LOG_ENTRY_ENCODED_LEN: usize = 25;
pub(super) const WAL_DURABILITY_UNCONFIRMED: u8 = 0;
pub(super) const WAL_DURABILITY_CONFIRMED: u8 = 1;

/// Unique identifier for a timestamp in the system.
pub type Timestamp = u64;

/// Entry in the commit log linking transaction ID to WAL durability.
///
/// Represents a transaction's commit state with proof of durability via WAL LSN.
#[derive(Debug, Clone)]
pub struct CommitLogEntry {
    /// Unique transaction identifier
    pub(super) tx_id: TransactionId,
    /// Log sequence number of the commit record in WAL
    pub(super) commit_lsn: Lsn,
    /// Timestamp when this version becomes visible to readers
    pub(super) visible_timestamp: Timestamp,
    /// Flag indicating WAL flush confirmation (atomic update)
    pub(super) wal_durability_confirmed: bool,
}

impl CommitLogEntry {
    /// Create a new commit log entry.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction identifier (must be non-zero)
    /// * `commit_lsn` - LSN of the commit record in WAL
    /// * `visible_timestamp` - Logical timestamp for visibility
    ///
    /// # Errors
    ///
    /// Returns error if `tx_id` is zero or if validation fails.
    ///
    /// # Invariant
    ///
    /// Entry is created with `wal_durability_confirmed = false`.
    /// Caller must invoke `mark_durable()` after WAL flush completes.
    pub fn new(
        tx_id: TransactionId,
        commit_lsn: Lsn,
        visible_timestamp: Timestamp,
    ) -> AndromedaResult<Self> {
        if tx_id.get() == 0 {
            return Err(transaction_error("transaction id must not be zero"));
        }

        if visible_timestamp == 0 {
            return Err(transaction_error("visible_timestamp must not be zero"));
        }

        if commit_lsn.is_zero() {
            return Err(transaction_error("commit_lsn must not be zero"));
        }

        Ok(CommitLogEntry {
            tx_id,
            commit_lsn,
            visible_timestamp,
            wal_durability_confirmed: false,
        })
    }

    /// Check if WAL durability is confirmed.
    ///
    /// Returns `true` only after `mark_durable()` has been called and completed.
    pub fn is_durable(&self) -> bool {
        self.wal_durability_confirmed
    }

    /// Mark this entry as durable (WAL flush confirmed).
    ///
    /// This operation is idempotent: calling it multiple times is safe.
    ///
    /// # Invariant
    ///
    /// This flag must be set BEFORE updating TransactionStatusTable to Committed.
    pub fn mark_durable(&mut self) {
        self.wal_durability_confirmed = true;
    }

    /// Encode entry to binary format for persistence.
    ///
    /// Format (25 bytes total):
    /// - Bytes 0-7:   `tx_id` (u64 little-endian)
    /// - Bytes 8-15:  `commit_lsn` (u64 little-endian)
    /// - Bytes 16-23: `visible_timestamp` (u64 little-endian)
    /// - Byte 24:     `wal_durability_confirmed` (0 or 1)
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(COMMIT_LOG_ENTRY_ENCODED_LEN);
        bytes.extend_from_slice(&self.tx_id.get().to_le_bytes());
        bytes.extend_from_slice(&self.commit_lsn.get().to_le_bytes());
        bytes.extend_from_slice(&self.visible_timestamp.to_le_bytes());
        bytes.push(if self.wal_durability_confirmed {
            WAL_DURABILITY_CONFIRMED
        } else {
            WAL_DURABILITY_UNCONFIRMED
        });
        bytes
    }

    /// Decode entry from binary format.
    ///
    /// # Errors
    ///
    /// Returns error if buffer is too small or contains invalid data.
    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        if bytes.len() < COMMIT_LOG_ENTRY_ENCODED_LEN {
            return Err(storage_error(format!(
                "CommitLogEntry buffer too small (need {} bytes)",
                COMMIT_LOG_ENTRY_ENCODED_LEN
            )));
        }

        let tx_id_raw = read_u64_field(bytes, TX_ID_OFFSET);
        if tx_id_raw == 0 {
            return Err(transaction_error("invalid transaction id in encoded entry"));
        }
        let tx_id = TransactionId::new(tx_id_raw);

        let commit_lsn_raw = read_u64_field(bytes, COMMIT_LSN_OFFSET);
        if commit_lsn_raw == 0 {
            return Err(transaction_error("invalid commit_lsn in encoded entry"));
        }
        let commit_lsn = Lsn::new(commit_lsn_raw);

        let visible_timestamp = read_u64_field(bytes, VISIBLE_TIMESTAMP_OFFSET);
        if visible_timestamp == 0 {
            return Err(transaction_error(
                "invalid visible_timestamp in encoded entry",
            ));
        }

        let wal_durability_confirmed = match bytes[DURABILITY_FLAG_OFFSET] {
            WAL_DURABILITY_UNCONFIRMED => false,
            WAL_DURABILITY_CONFIRMED => true,
            flag => {
                return Err(transaction_error(format!(
                    "invalid WAL durability flag in encoded entry: {flag}"
                )));
            }
        };

        Ok(CommitLogEntry {
            tx_id,
            commit_lsn,
            visible_timestamp,
            wal_durability_confirmed,
        })
    }

    /// Get the transaction ID.
    pub fn tx_id(&self) -> TransactionId {
        self.tx_id
    }

    /// Get the commit LSN.
    pub fn commit_lsn(&self) -> Lsn {
        self.commit_lsn
    }

    /// Get the visible timestamp.
    pub fn visible_timestamp(&self) -> Timestamp {
        self.visible_timestamp
    }
}

fn read_u64_field(bytes: &[u8], offset: usize) -> u64 {
    let mut raw = [0; U64_FIELD_BYTES];
    raw.copy_from_slice(&bytes[offset..offset + U64_FIELD_BYTES]);
    u64::from_le_bytes(raw)
}
