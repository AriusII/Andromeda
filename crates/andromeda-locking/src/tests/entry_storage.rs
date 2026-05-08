use super::*;

#[test]
fn manager_tracks_entries_and_waiters() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(11);

    manager.ensure_entry(resource).unwrap();
    assert_eq!(manager.entry_count().unwrap(), 1);

    let sequence = manager
        .enqueue_waiter(resource, tx_id, LockMode::Exclusive)
        .unwrap();
    assert_eq!(sequence, 1);

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 0);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, tx_id);
}
