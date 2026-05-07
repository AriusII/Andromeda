use super::*;
use crate::Lsn;
use andromeda_core::TransactionId;
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
fn test_commit_log_record_commit_duplicate_is_atomic() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    let commit_log = Arc::new(CommitLog::new());
    let start = Arc::new(Barrier::new(2));
    let tx_id = TransactionId::new(10_000);
    let successes = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = [Lsn::new(100), Lsn::new(200)]
        .into_iter()
        .map(|commit_lsn| {
            let log = Arc::clone(&commit_log);
            let start = Arc::clone(&start);
            let successes = Arc::clone(&successes);
            let failures = Arc::clone(&failures);

            thread::spawn(move || {
                let entry = CommitLogEntry::new(tx_id, commit_lsn, commit_lsn.get()).unwrap();
                start.wait();
                match log.record_commit(entry) {
                    Ok(()) => {
                        successes.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(successes.load(Ordering::Relaxed), 1);
    assert_eq!(failures.load(Ordering::Relaxed), 1);
    assert_eq!(commit_log.entry_count(), 1);

    let stored = commit_log.query_commit_status(tx_id).unwrap().unwrap();
    assert!(!stored.is_durable());
    assert!([Lsn::new(100), Lsn::new(200)].contains(&stored.commit_lsn()));
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
