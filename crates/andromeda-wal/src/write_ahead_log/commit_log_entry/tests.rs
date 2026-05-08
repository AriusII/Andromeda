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
fn commit_log_entry_encode_decode_roundtrip() {
    let tx_id = TransactionIdGenerator::new().generate();
    let mut entry = CommitLogEntry::new(tx_id, Lsn::new(12_345), 67_890).unwrap();
    entry.mark_durable();

    let encoded = entry.encode();
    assert_eq!(encoded.len(), COMMIT_LOG_ENTRY_ENCODED_LEN);
    assert_eq!(encoded[DURABILITY_FLAG_OFFSET], WAL_DURABILITY_CONFIRMED);

    let decoded = CommitLogEntry::decode(&encoded).unwrap();
    assert_eq!(decoded.tx_id(), entry.tx_id());
    assert_eq!(decoded.commit_lsn(), entry.commit_lsn());
    assert_eq!(decoded.visible_timestamp(), entry.visible_timestamp());
    assert_eq!(decoded.is_durable(), entry.is_durable());
}

#[test]
fn commit_log_entry_encode_tracks_unconfirmed_durability() {
    let tx_id = TransactionIdGenerator::new().generate();
    let entry = CommitLogEntry::new(tx_id, Lsn::new(100), 500).unwrap();

    let encoded = entry.encode();
    assert_eq!(encoded[DURABILITY_FLAG_OFFSET], WAL_DURABILITY_UNCONFIRMED);
    assert!(!CommitLogEntry::decode(&encoded).unwrap().is_durable());
}

#[test]
fn commit_log_rejects_duplicate_commit_atomically() {
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
fn commit_log_cleanup_only_removes_durable_old_entries() {
    let commit_log = CommitLog::new();
    let tx_id_1 = TransactionIdGenerator::new().generate();
    let tx_id_2 = TransactionIdGenerator::new().generate();

    commit_log
        .record_commit(CommitLogEntry::new(tx_id_1, Lsn::new(50), 500).unwrap())
        .unwrap();
    commit_log
        .record_commit(CommitLogEntry::new(tx_id_2, Lsn::new(100), 600).unwrap())
        .unwrap();
    commit_log.confirm_durable(tx_id_1).unwrap();

    assert_eq!(commit_log.cleanup_entries(Lsn::new(150)), 1);
    assert_eq!(commit_log.entry_count(), 1);
    assert!(commit_log.query_commit_status(tx_id_2).unwrap().is_some());
}
