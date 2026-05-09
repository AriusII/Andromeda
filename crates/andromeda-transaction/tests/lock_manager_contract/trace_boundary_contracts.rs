use super::fixtures::{assert_granted, table_resource};
use super::*;

#[test]
fn lock_trace_evidence_does_not_carry_durability_or_wal_claims() {
    let manager = LockManager::new();
    let resource = table_resource(75);
    let holder = TransactionId::new(79);
    let waiter = TransactionId::new(80);

    assert_granted(&manager, holder, resource, LockMode::Exclusive);
    let wait = manager
        .acquire_with_evidence(waiter, resource, LockMode::Shared)
        .unwrap()
        .evidence
        .unwrap();
    let release_all = manager.release_all_with_evidence(holder).unwrap();
    let rendered = format!("{wait:?} {:?}", release_all.evidence);
    let rendered = rendered.to_ascii_lowercase();

    assert!(!rendered.contains("durable_lsn"));
    assert!(!rendered.contains("wal"));
    assert!(!rendered.contains("commit"));
}
