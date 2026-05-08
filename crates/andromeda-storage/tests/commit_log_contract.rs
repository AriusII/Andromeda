//! Comprehensive contract tests for commit log entry and commit log.
//!
//! Verifies correctness of WAL durability tracking, encoding/decoding, and lifecycle.

use andromeda_core::TransactionId;
use andromeda_wal::Lsn;
use andromeda_wal::write_ahead_log::{CommitLog, CommitLogEntry, CommitLogFacade};
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

// CommitLogEntry Creation and Validation

#[test]
fn test_commit_log_entry_creation_valid() {
    let tx_id = TransactionIdGenerator::new().generate();
    let lsn = Lsn::new(100);
    let visible_ts = 500;

    let entry = CommitLogEntry::new(tx_id, lsn, visible_ts).unwrap();

    assert_eq!(entry.tx_id(), tx_id);
    assert_eq!(entry.commit_lsn(), lsn);
    assert_eq!(entry.visible_timestamp(), visible_ts);
    assert!(!entry.is_durable()); // Initially not durable
}

#[test]
fn test_commit_log_entry_invalid_tx_id_zero() {
    let result = CommitLogEntry::new(TransactionId::new(0), Lsn::new(100), 500);
    assert!(result.is_err());
}

#[test]
fn test_commit_log_entry_invalid_visible_timestamp_zero() {
    let tx_id = TransactionIdGenerator::new().generate();
    let result = CommitLogEntry::new(tx_id, Lsn::new(100), 0);
    assert!(result.is_err());
}

#[test]
fn test_commit_log_entry_invalid_commit_lsn_zero() {
    let tx_id = TransactionIdGenerator::new().generate();
    let result = CommitLogEntry::new(tx_id, Lsn::ZERO, 500);
    assert!(result.is_err());
}

// Durability Flag Lifecycle

#[test]
fn test_commit_log_entry_initially_not_durable() {
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    assert!(!entry.is_durable());
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
fn test_commit_log_entry_mark_durable_idempotent() {
    let tx_id = TransactionIdGenerator::new().generate();
    let mut entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    entry.mark_durable();
    assert!(entry.is_durable());

    entry.mark_durable(); // Call again
    assert!(entry.is_durable()); // Should remain durable
}

// Encoding and Decoding

#[test]
fn test_commit_log_entry_encode_size() {
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    let encoded = entry.encode();
    assert_eq!(encoded.len(), 25);
}

#[test]
fn test_commit_log_entry_encode_decode_roundtrip() {
    let tx_id = TransactionIdGenerator::new().generate();
    let lsn = Lsn::new(12345);
    let visible_ts = 67890;

    let mut original = CommitLogEntry::new(tx_id, lsn, visible_ts).unwrap();
    original.mark_durable();

    let encoded = original.encode();
    let decoded = CommitLogEntry::decode(&encoded).unwrap();

    assert_eq!(decoded.tx_id(), original.tx_id());
    assert_eq!(decoded.commit_lsn(), original.commit_lsn());
    assert_eq!(decoded.visible_timestamp(), original.visible_timestamp());
    assert_eq!(decoded.is_durable(), original.is_durable());
}

#[test]
fn test_commit_log_entry_encode_decode_not_durable() {
    let tx_id = TransactionIdGenerator::new().generate();
    let original = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    let encoded = original.encode();
    assert_eq!(encoded[24], 0u8); // Durability flag is false

    let decoded = CommitLogEntry::decode(&encoded).unwrap();
    assert!(!decoded.is_durable());
}

#[test]
fn test_commit_log_entry_encode_decode_durable() {
    let tx_id = TransactionIdGenerator::new().generate();
    let mut original = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
    original.mark_durable();

    let encoded = original.encode();
    assert_eq!(encoded[24], 1u8); // Durability flag is true

    let decoded = CommitLogEntry::decode(&encoded).unwrap();
    assert!(decoded.is_durable());
}

#[test]
fn test_commit_log_entry_decode_buffer_too_small() {
    let buffer = vec![0u8; 24]; // One byte short
    let result = CommitLogEntry::decode(&buffer);
    assert!(result.is_err());
}

#[test]
fn test_commit_log_entry_decode_invalid_tx_id() {
    let buffer = vec![0u8; 25];
    // All zeros would result in tx_id = 0, which is invalid
    let result = CommitLogEntry::decode(&buffer);
    assert!(result.is_err());
}

#[test]
fn test_commit_log_entry_decode_rejects_invalid_durable_evidence() {
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
    let mut encoded = entry.encode();

    encoded[8..16].copy_from_slice(&0u64.to_le_bytes());
    let error = CommitLogEntry::decode(&encoded)
        .expect_err("zero commit LSN must be rejected during decode");
    assert!(error.message().contains("commit_lsn"));

    let mut encoded = entry.encode();
    encoded[16..24].copy_from_slice(&0u64.to_le_bytes());
    let error = CommitLogEntry::decode(&encoded)
        .expect_err("zero visible timestamp must be rejected during decode");
    assert!(error.message().contains("visible_timestamp"));

    let mut encoded = entry.encode();
    encoded[24] = 2;
    let error = CommitLogEntry::decode(&encoded)
        .expect_err("non-binary durability flag must be rejected during decode");
    assert!(error.message().contains("durability flag"));
}

// CommitLog Basic Operations

#[test]
fn test_commit_log_new_empty() {
    let commit_log = CommitLog::new();
    assert_eq!(commit_log.entry_count(), 0);
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
fn test_commit_log_record_commit_duplicate_rejected() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();

    let entry1 = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
    assert!(commit_log.record_commit(entry1).is_ok());

    let entry2 = CommitLogEntry::new(tx_id, Lsn::new(200), 600).unwrap();
    assert!(commit_log.record_commit(entry2).is_err()); // Duplicate
}

#[test]
fn test_commit_log_record_ensures_not_durable() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();
    let mut entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
    entry.mark_durable();

    commit_log.record_commit(entry).unwrap();

    let queried = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    // record_commit resets the flag to ensure initial non-durable state
    assert!(!queried.is_durable());
}

// Query Operations

#[test]
fn test_commit_log_query_found() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry.clone()).unwrap();

    let queried = commit_log.query_commit_status(tx_id).unwrap();
    assert!(queried.is_some());
    let queried_entry = queried.unwrap();
    assert_eq!(queried_entry.tx_id(), tx_id);
}

#[test]
fn test_commit_log_query_not_found() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();

    let queried = commit_log.query_commit_status(tx_id).unwrap();
    assert!(queried.is_none());
}

#[test]
fn test_commit_log_query_returns_copy() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry).unwrap();

    let queried1 = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    let queried2 = commit_log.query_commit_status(tx_id).unwrap().unwrap();

    assert_eq!(queried1.tx_id(), queried2.tx_id());
}

// Durability Confirmation

#[test]
fn test_commit_log_confirm_durable() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry).unwrap();

    let before = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(!before.is_durable());

    commit_log.confirm_durable(tx_id).unwrap();

    let after = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(after.is_durable());
}

#[test]
fn test_commit_log_confirm_durable_not_found() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();

    let result = commit_log.confirm_durable(tx_id);
    assert!(result.is_err());
}

#[test]
fn test_commit_log_confirm_durable_idempotent() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry).unwrap();
    commit_log.confirm_durable(tx_id).unwrap();
    commit_log.confirm_durable(tx_id).unwrap(); // Call again

    let result = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(result.is_durable());
}

// Cleanup Operations

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

    // Confirm all as durable
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
fn test_commit_log_cleanup_no_matches() {
    let commit_log = CommitLog::new();

    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry).unwrap();
    commit_log.confirm_durable(tx_id).unwrap();

    // Clean up with LSN less than entry's LSN
    let removed = commit_log.cleanup_entries(Lsn::new(50));
    assert_eq!(removed, 0);
    assert_eq!(commit_log.entry_count(), 1);
}

#[test]
fn test_commit_log_cleanup_boundary() {
    let commit_log = CommitLog::new();

    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry).unwrap();
    commit_log.confirm_durable(tx_id).unwrap();

    // Clean up with LSN exactly at entry's LSN (should not remove)
    let removed = commit_log.cleanup_entries(Lsn::new(100));
    assert_eq!(removed, 0);
    assert_eq!(commit_log.entry_count(), 1);

    // Clean up with LSN one above entry's LSN (should remove)
    let removed = commit_log.cleanup_entries(Lsn::new(101));
    assert_eq!(removed, 1);
    assert_eq!(commit_log.entry_count(), 0);
}

#[test]
fn test_commit_log_default_construction() {
    let log1 = CommitLog::default();
    let log2 = CommitLog::new();

    assert_eq!(log1.entry_count(), 0);
    assert_eq!(log2.entry_count(), 0);
}

// Clone and Copy Semantics

#[test]
fn test_commit_log_entry_clone() {
    let tx_id = TransactionIdGenerator::new().generate();
    let mut entry1 = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
    entry1.mark_durable();

    let entry2 = entry1.clone();

    assert_eq!(entry1.tx_id(), entry2.tx_id());
    assert_eq!(entry1.commit_lsn(), entry2.commit_lsn());
    assert_eq!(entry1.is_durable(), entry2.is_durable());
}

// Concurrent Operations (Thread Safety)

#[test]
fn test_commit_log_concurrent_record_and_query() {
    use std::sync::Arc;
    use std::thread;

    let commit_log = Arc::new(CommitLog::new());
    let mut handles = vec![];

    // Spawn 10 threads, each recording and confirming a commit
    for i in 0..10 {
        let log = Arc::clone(&commit_log);
        let handle = thread::spawn(move || {
            let tx_id = TransactionIdGenerator::new().generate();
            let entry = CommitLogEntry::new(tx_id, Lsn::new(100 + i), 500).unwrap();
            log.record_commit(entry).unwrap();
            log.confirm_durable(tx_id).unwrap();
            tx_id
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(commit_log.entry_count(), 10);
}

#[test]
fn test_commit_log_concurrent_cleanup() {
    use std::sync::Arc;
    use std::thread;

    let commit_log = Arc::new(CommitLog::new());

    // Pre-populate with entries
    for i in 0..10 {
        let tx_id = TransactionIdGenerator::new().generate();
        let entry = CommitLogEntry::new(tx_id, Lsn::new(50 + i * 10), 500).unwrap();
        commit_log.record_commit(entry).unwrap();
        commit_log.confirm_durable(tx_id).unwrap();
    }

    assert_eq!(commit_log.entry_count(), 10);

    // Spawn cleanup threads
    let log = Arc::clone(&commit_log);
    let handle1 = thread::spawn(move || log.cleanup_entries(Lsn::new(80)));

    let log = Arc::clone(&commit_log);
    let handle2 = thread::spawn(move || log.cleanup_entries(Lsn::new(100)));

    let _removed1 = handle1.join().unwrap();
    let _removed2 = handle2.join().unwrap();
}

// Durability Invariant Tests

#[test]
fn test_invariant_entry_never_committed_until_durable() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    commit_log.record_commit(entry).unwrap();

    // Entry should not be durable immediately after recording
    let recorded = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(!recorded.is_durable());

    // Only after confirmation should it be durable
    commit_log.confirm_durable(tx_id).unwrap();
    let confirmed = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(confirmed.is_durable());
}

#[test]
fn test_invariant_cleanup_only_removes_durable() {
    let commit_log = CommitLog::new();

    let tx_id_not_durable = TransactionIdGenerator::new().generate();
    let tx_id_durable = TransactionIdGenerator::new().generate();

    let entry1 = CommitLogEntry::new(tx_id_not_durable, Lsn::new(50), 500).unwrap();
    let entry2 = CommitLogEntry::new(tx_id_durable, Lsn::new(50), 500).unwrap();

    commit_log.record_commit(entry1).unwrap();
    commit_log.record_commit(entry2).unwrap();

    // Only entry2 is durable
    commit_log.confirm_durable(tx_id_durable).unwrap();

    // Clean up all before LSN 100
    let removed = commit_log.cleanup_entries(Lsn::new(100));

    // Only 1 should be removed (the durable one)
    assert_eq!(removed, 1);

    // Non-durable entry should remain
    assert!(
        commit_log
            .query_commit_status(tx_id_not_durable)
            .unwrap()
            .is_some()
    );
}

#[test]
fn test_commit_workflow_happy_path() {
    let commit_log = CommitLog::new();
    let tx_id = TransactionIdGenerator::new().generate();

    // Step 1: Record commit
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();
    commit_log.record_commit(entry).unwrap();

    // Step 2: Verify not yet durable
    let recorded = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(!recorded.is_durable());

    // Step 3: Confirm durability
    commit_log.confirm_durable(tx_id).unwrap();

    // Step 4: Verify now durable
    let confirmed = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(confirmed.is_durable());

    // Step 5: After backup, cleanup old entries
    let removed = commit_log.cleanup_entries(Lsn::new(150));
    assert_eq!(removed, 1);

    // Step 6: Verify entry is gone
    assert!(commit_log.query_commit_status(tx_id).unwrap().is_none());
}

// CommitLogFacade Visibility Gate

#[test]
fn test_facade_rejects_visibility_before_wal_durability() {
    let facade = CommitLogFacade::new();
    let tx_id = TransactionIdGenerator::new().generate();

    facade.record_commit(tx_id, Lsn::new(100), 500).unwrap();

    let error = facade
        .make_visible(tx_id)
        .expect_err("visibility must be blocked until WAL durability is confirmed");
    assert!(error.message().contains("not durable"));
}

#[test]
fn test_facade_allows_visibility_after_wal_durability() {
    let facade = CommitLogFacade::new();
    let tx_id = TransactionIdGenerator::new().generate();

    facade.record_commit(tx_id, Lsn::new(100), 500).unwrap();
    facade.confirm_durable(tx_id).unwrap();

    facade
        .make_visible(tx_id)
        .expect("durable WAL evidence must allow visibility publication");
}

#[test]
fn test_facade_cleanup_removes_only_durable_entries() {
    let facade = CommitLogFacade::new();
    let durable_tx = TransactionIdGenerator::new().generate();
    let pending_tx = TransactionIdGenerator::new().generate();

    facade
        .record_commit(durable_tx, Lsn::new(100), 500)
        .unwrap();
    facade
        .record_commit(pending_tx, Lsn::new(110), 510)
        .unwrap();
    facade.confirm_durable(durable_tx).unwrap();

    assert_eq!(facade.cleanup_before_lsn(Lsn::new(200)), 1);
    assert!(facade.query_status(durable_tx).unwrap().is_none());
    assert!(facade.query_status(pending_tx).unwrap().is_some());
}
