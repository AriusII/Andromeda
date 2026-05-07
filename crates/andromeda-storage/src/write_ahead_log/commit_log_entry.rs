//! Commit Log Entry Model and Persistence Layer
//!
//! This module defines the type system for commit log entries and their lifecycle,
//! linking transaction commits to WAL durability evidence via LSN tracking.
//!
//! # Durability Invariants
//!
//! **Invariant 1: Commit log entry created for an assigned WAL commit LSN**
//! - Entry cannot exist before the caller has assigned the corresponding WAL commit LSN
//! - The caller owns the WAL append boundary; this type records the durable-ordering evidence
//!
//! **Invariant 2: Durable flag set AFTER WAL flush confirmation**
//! - Flag must remain `false` until WAL flush completes
//! - Flipping from `false` → `true` is atomic operation
//! - Never flipped back to `false`
//!
//! **Invariant 3: Never mark visible until durable flag is true**
//! - TransactionStatusTable update deferred until `is_durable() == true`
//! - Ensures "visible commit ≡ durable WAL" doctrine
//!
//! **Invariant 4: GC Eligibility based on LSN**
//! - Entries with `commit_lsn < min_active_snapshot_lsn` are GC candidates
//! - Cleanup only after durability confirmation and visibility update
//!
//! # Thread Safety
//!
//! `CommitLog` uses DashMap for concurrent access without locking entire table.
//! Per-entry durability transitions are made through DashMap entry mutation.
//!
//! # Encoding Format
//!
//! Binary format for persistence (25 bytes total, little-endian integer fields):
//! - Bytes 0-7:   `tx_id` (u64)
//! - Bytes 8-15:  `commit_lsn` (u64)
//! - Bytes 16-23: `visible_timestamp` (u64)
//! - Byte 24:     `wal_durability_confirmed` (u8: 0 or 1)

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use dashmap::DashMap;
use std::sync::Arc;

use crate::Lsn;

const U64_FIELD_BYTES: usize = 8;
const TX_ID_OFFSET: usize = 0;
const COMMIT_LSN_OFFSET: usize = 8;
const VISIBLE_TIMESTAMP_OFFSET: usize = 16;
const DURABILITY_FLAG_OFFSET: usize = 24;
const COMMIT_LOG_ENTRY_ENCODED_LEN: usize = 25;
const WAL_DURABILITY_UNCONFIRMED: u8 = 0;
const WAL_DURABILITY_CONFIRMED: u8 = 1;

/// Unique identifier for a timestamp in the system.
pub type Timestamp = u64;

/// Entry in the commit log linking transaction ID to WAL durability.
///
/// Represents a transaction's commit state with proof of durability via WAL LSN.
#[derive(Debug, Clone)]
pub struct CommitLogEntry {
    /// Unique transaction identifier
    tx_id: TransactionId,
    /// Log sequence number of the commit record in WAL
    commit_lsn: Lsn,
    /// Timestamp when this version becomes visible to readers
    visible_timestamp: Timestamp,
    /// Flag indicating WAL flush confirmation (atomic update)
    wal_durability_confirmed: bool,
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

/// Commit log: in-memory snapshot cache with persistence support.
///
/// Maintains a fast lookup table of committed transactions with their
/// WAL durability evidence. Primary storage is the WAL; this structure
/// is a snapshot cache rebuilt on recovery.
pub struct CommitLog {
    /// Fast lookup: TxId → CommitLogEntry
    entries: Arc<DashMap<TransactionId, CommitLogEntry>>,
}

impl CommitLog {
    /// Create a new empty commit log.
    pub fn new() -> Self {
        CommitLog {
            entries: Arc::new(DashMap::new()),
        }
    }

    /// Record a commit: log entry and (atomically) mark to WAL.
    ///
    /// # Invariant
    ///
    /// Entry is stored with `wal_durability_confirmed = false` initially.
    /// Caller MUST call `confirm_durable()` after WAL flush completes.
    ///
    /// # Arguments
    ///
    /// * `entry` - Commit log entry to record
    ///
    /// # Errors
    ///
    /// Returns error if entry already exists for this transaction ID.
    pub fn record_commit(&self, mut entry: CommitLogEntry) -> AndromedaResult<()> {
        // Ensure entry is created with durability NOT yet confirmed
        entry.wal_durability_confirmed = false;

        if self.entries.contains_key(&entry.tx_id) {
            return Err(transaction_error(format!(
                "commit log entry already exists for transaction {}",
                entry.tx_id.get()
            )));
        }

        self.entries.insert(entry.tx_id, entry);
        Ok(())
    }

    /// Confirm durability for a commit entry.
    ///
    /// This MUST be called after WAL flush completes to atomically
    /// flip the `wal_durability_confirmed` flag.
    ///
    /// # Invariant
    ///
    /// After this call returns, `query_commit_status()` will return
    /// an entry with `is_durable() == true`.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction to mark as durable
    ///
    /// # Errors
    ///
    /// Returns error if no entry exists for this transaction ID.
    pub fn confirm_durable(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        match self.entries.get_mut(&tx_id) {
            Some(mut entry) => {
                entry.mark_durable();
                Ok(())
            }
            None => Err(transaction_error(format!(
                "commit log entry not found for transaction {}",
                tx_id.get()
            ))),
        }
    }

    /// Query commit status for a transaction.
    ///
    /// # Returns
    ///
    /// `Some(entry)` if transaction has a commit log entry, `None` otherwise.
    ///
    /// # Errors
    ///
    /// No errors: always returns successfully (entry may not exist).
    pub fn query_commit_status(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<Option<CommitLogEntry>> {
        Ok(self.entries.get(&tx_id).map(|ref_multi| ref_multi.clone()))
    }

    /// Clean up entries before a given LSN (for GC after backup).
    ///
    /// Removes all entries with `commit_lsn < before_lsn` that have
    /// durability confirmed. This is safe because:
    /// 1. Visibility has already been registered with TransactionStatusTable
    /// 2. Old commits are not needed for future WAL recovery
    ///
    /// # Returns
    ///
    /// Number of entries cleaned up.
    pub fn cleanup_entries(&self, before_lsn: Lsn) -> usize {
        let mut count = 0;

        // Collect IDs to remove (to avoid holding lock during iteration)
        let to_remove: Vec<TransactionId> = self
            .entries
            .iter()
            .filter(|ref_multi| {
                let entry = ref_multi.value();
                entry.commit_lsn < before_lsn && entry.is_durable()
            })
            .map(|ref_multi| *ref_multi.key())
            .collect();

        for tx_id in to_remove {
            if self.entries.remove(&tx_id).is_some() {
                count += 1;
            }
        }

        count
    }

    /// Get count of entries currently in the log.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get all transaction IDs in the log.
    ///
    /// Used for testing and diagnostics.
    pub fn all_tx_ids(&self) -> Vec<TransactionId> {
        self.entries
            .iter()
            .map(|ref_multi| *ref_multi.key())
            .collect()
    }

    /// Clear all entries (for testing).
    pub fn clear(&self) {
        self.entries.clear();
    }
}

fn read_u64_field(bytes: &[u8], offset: usize) -> u64 {
    let mut raw = [0; U64_FIELD_BYTES];
    raw.copy_from_slice(&bytes[offset..offset + U64_FIELD_BYTES]);
    u64::from_le_bytes(raw)
}

fn storage_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}

fn transaction_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transaction, msg)
}

impl Default for CommitLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TransactionIdGenerator;

    impl TransactionIdGenerator {
        fn new() -> Self {
            Self
        }

        fn generate(&self) -> TransactionId {
            static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);
            TransactionId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
        }
    }

    #[test]
    fn test_commit_log_entry_creation() {
        let tx_id = TransactionIdGenerator::new().generate();
        let lsn = Lsn::new(100);
        let ts = 500;

        let entry = CommitLogEntry::new(tx_id, lsn, ts).unwrap();

        assert_eq!(entry.tx_id(), tx_id);
        assert_eq!(entry.commit_lsn(), lsn);
        assert_eq!(entry.visible_timestamp(), ts);
        assert!(!entry.is_durable()); // Initially not durable
    }

    #[test]
    fn test_commit_log_entry_invalid_tx_id() {
        let result = CommitLogEntry::new(TransactionId::new(0), Lsn::new(100), 500);
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_log_entry_invalid_timestamp() {
        let tx_id = TransactionIdGenerator::new().generate();
        let result = CommitLogEntry::new(tx_id, Lsn::new(100), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_log_entry_mark_durable() {
        let tx_id = TransactionIdGenerator::new().generate();
        let mut entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

        assert!(!entry.is_durable());
        entry.mark_durable();
        assert!(entry.is_durable());
    }

    #[test]
    fn test_commit_log_entry_encode_decode_roundtrip() {
        let tx_id = TransactionIdGenerator::new().generate();
        let lsn = Lsn::new(12345);
        let ts = 67890;

        let mut entry = CommitLogEntry::new(tx_id, lsn, ts).unwrap();
        entry.mark_durable();

        let encoded = entry.encode();
        assert_eq!(encoded.len(), COMMIT_LOG_ENTRY_ENCODED_LEN);

        let decoded = CommitLogEntry::decode(&encoded).unwrap();
        assert_eq!(decoded.tx_id(), entry.tx_id());
        assert_eq!(decoded.commit_lsn(), entry.commit_lsn());
        assert_eq!(decoded.visible_timestamp(), entry.visible_timestamp());
        assert_eq!(decoded.is_durable(), entry.is_durable());
    }

    #[test]
    fn test_commit_log_entry_decode_invalid_size() {
        let result = CommitLogEntry::decode(&[0u8; 24]); // Too small
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_log_entry_encode_not_durable() {
        let tx_id = TransactionIdGenerator::new().generate();
        let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

        let encoded = entry.encode();
        assert_eq!(encoded[DURABILITY_FLAG_OFFSET], WAL_DURABILITY_UNCONFIRMED);

        let decoded = CommitLogEntry::decode(&encoded).unwrap();
        assert!(!decoded.is_durable());
    }

    #[test]
    fn test_commit_log_entry_encode_durable() {
        let tx_id = TransactionIdGenerator::new().generate();
        let mut entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
        entry.mark_durable();

        let encoded = entry.encode();
        assert_eq!(encoded[DURABILITY_FLAG_OFFSET], WAL_DURABILITY_CONFIRMED);
    }

    #[test]
    fn test_commit_log_record_commit() {
        let commit_log = CommitLog::new();
        let tx_id = TransactionIdGenerator::new().generate();
        let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

        assert!(commit_log.record_commit(entry).is_ok());
        assert_eq!(commit_log.entry_count(), 1);
    }

    #[test]
    fn test_commit_log_record_commit_duplicate() {
        let commit_log = CommitLog::new();
        let tx_id = TransactionIdGenerator::new().generate();
        let entry1 = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
        let entry2 = CommitLogEntry::new(tx_id, Lsn::new(200), 600).unwrap();

        assert!(commit_log.record_commit(entry1).is_ok());
        assert!(commit_log.record_commit(entry2).is_err()); // Duplicate
    }

    #[test]
    fn test_commit_log_query_commit_status() {
        let commit_log = CommitLog::new();
        let tx_id = TransactionIdGenerator::new().generate();
        let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

        commit_log.record_commit(entry.clone()).unwrap();

        let queried = commit_log.query_commit_status(tx_id).unwrap();
        assert!(queried.is_some());
        assert_eq!(queried.unwrap().tx_id(), tx_id);
    }

    #[test]
    fn test_commit_log_query_not_found() {
        let commit_log = CommitLog::new();
        let tx_id = TransactionIdGenerator::new().generate();

        let queried = commit_log.query_commit_status(tx_id).unwrap();
        assert!(queried.is_none());
    }

    #[test]
    fn test_commit_log_confirm_durable() {
        let commit_log = CommitLog::new();
        let tx_id = TransactionIdGenerator::new().generate();
        let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

        commit_log.record_commit(entry).unwrap();

        let before_confirm = commit_log.query_commit_status(tx_id).unwrap().unwrap();
        assert!(!before_confirm.is_durable());

        commit_log.confirm_durable(tx_id).unwrap();

        let after_confirm = commit_log.query_commit_status(tx_id).unwrap().unwrap();
        assert!(after_confirm.is_durable());
    }

    #[test]
    fn test_commit_log_confirm_durable_not_found() {
        let commit_log = CommitLog::new();
        let tx_id = TransactionIdGenerator::new().generate();

        let result = commit_log.confirm_durable(tx_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_log_cleanup_entries() {
        let commit_log = CommitLog::new();

        let tx_id_1 = TransactionIdGenerator::new().generate();
        let tx_id_2 = TransactionIdGenerator::new().generate();
        let tx_id_3 = TransactionIdGenerator::new().generate();

        let entry1 = CommitLogEntry::new(tx_id_1, Lsn::new(50), 500).unwrap();
        let entry2 = CommitLogEntry::new(tx_id_2, Lsn::new(100), 600).unwrap();
        let entry3 = CommitLogEntry::new(tx_id_3, Lsn::new(200), 700).unwrap();

        commit_log.record_commit(entry1).unwrap();
        commit_log.record_commit(entry2).unwrap();
        commit_log.record_commit(entry3).unwrap();

        // Confirm durability for all
        commit_log.confirm_durable(tx_id_1).unwrap();
        commit_log.confirm_durable(tx_id_2).unwrap();
        commit_log.confirm_durable(tx_id_3).unwrap();

        assert_eq!(commit_log.entry_count(), 3);

        // Clean up entries before LSN 150
        let removed = commit_log.cleanup_entries(Lsn::new(150));
        assert_eq!(removed, 2); // entry1 (LSN 50) and entry2 (LSN 100)
        assert_eq!(commit_log.entry_count(), 1); // Only entry3 remains
    }

    #[test]
    fn test_commit_log_cleanup_skips_non_durable() {
        let commit_log = CommitLog::new();

        let tx_id_1 = TransactionIdGenerator::new().generate();
        let tx_id_2 = TransactionIdGenerator::new().generate();

        let entry1 = CommitLogEntry::new(tx_id_1, Lsn::new(50), 500).unwrap();
        let entry2 = CommitLogEntry::new(tx_id_2, Lsn::new(100), 600).unwrap();

        commit_log.record_commit(entry1).unwrap();
        commit_log.record_commit(entry2).unwrap();

        // Confirm durability only for entry1
        commit_log.confirm_durable(tx_id_1).unwrap();
        // entry2 is NOT confirmed durable

        assert_eq!(commit_log.entry_count(), 2);

        // Clean up entries before LSN 150
        let removed = commit_log.cleanup_entries(Lsn::new(150));
        assert_eq!(removed, 1); // Only entry1 (durable)
        assert_eq!(commit_log.entry_count(), 1); // entry2 remains (not durable)
    }

    #[test]
    fn test_commit_log_all_tx_ids() {
        let commit_log = CommitLog::new();

        let tx_ids: Vec<_> = (0..5)
            .map(|_| {
                let tx_id = TransactionIdGenerator::new().generate();
                let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
                commit_log.record_commit(entry).unwrap();
                tx_id
            })
            .collect();

        let stored_ids = commit_log.all_tx_ids();
        assert_eq!(stored_ids.len(), 5);

        for tx_id in tx_ids {
            assert!(stored_ids.contains(&tx_id));
        }
    }

    #[test]
    fn test_commit_log_clear() {
        let commit_log = CommitLog::new();

        for _ in 0..5 {
            let tx_id = TransactionIdGenerator::new().generate();
            let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
            commit_log.record_commit(entry).unwrap();
        }

        assert!(commit_log.entry_count() > 0);
        commit_log.clear();
        assert_eq!(commit_log.entry_count(), 0);
    }

    #[test]
    fn test_commit_log_default_construction() {
        let log1 = CommitLog::default();
        let log2 = CommitLog::new();

        assert_eq!(log1.entry_count(), 0);
        assert_eq!(log2.entry_count(), 0);
    }

    #[test]
    fn test_commit_log_entry_clone() {
        let tx_id = TransactionIdGenerator::new().generate();
        let mut entry1 = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
        entry1.mark_durable();

        let entry2 = entry1.clone();

        assert_eq!(entry1.tx_id(), entry2.tx_id());
        assert_eq!(entry1.is_durable(), entry2.is_durable());
    }

    #[test]
    fn test_commit_log_concurrent_operations() {
        use std::sync::Arc;
        use std::thread;

        let commit_log = Arc::new(CommitLog::new());
        let mut handles = vec![];

        // Spawn 10 threads, each recording a commit
        for _ in 0..10 {
            let log = Arc::clone(&commit_log);
            let handle = thread::spawn(move || {
                let tx_id = TransactionIdGenerator::new().generate();
                let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
                log.record_commit(entry).unwrap();
                log.confirm_durable(tx_id).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(commit_log.entry_count(), 10);
    }
}
