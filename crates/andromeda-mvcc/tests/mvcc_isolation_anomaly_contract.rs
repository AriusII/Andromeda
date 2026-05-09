//! MVCC isolation anomaly labels for the V0 public transaction contract.
//!
//! These tests describe the behavior that exists today. `Serializable` is a
//! commit-log metadata label in `andromeda-transaction-log`; it is not treated
//! here as a proof of serializable execution.

use andromeda_time::EngineTimestamp;

use andromeda_mvcc::{
    ActiveSnapshotRegistry, MvccIsolationPolicy, MvccRowHeader, Snapshot, SnapshotHandle,
    TransactionStatus, TransactionStatusTable,
};
use andromeda_transaction_log::{CommitLogEntry, IsolationLevel, Lsn};
use andromeda_types::{CatalogVersion, TransactionId};

fn tx(id: u64) -> TransactionId {
    TransactionId::new(id)
}

fn mark_committed(statuses: &TransactionStatusTable, tx_id: TransactionId, lsn: u64) {
    statuses
        .record_committed_after_durable_wal(tx_id, Lsn::new(lsn), Lsn::new(lsn))
        .unwrap();
}

fn mark_rolled_back(statuses: &TransactionStatusTable, tx_id: TransactionId, lsn: u64) {
    statuses
        .record_rolled_back_after_durable_wal(tx_id, Lsn::new(lsn), Lsn::new(lsn))
        .unwrap();
}

fn snapshot(
    timestamp: u64,
    policy: MvccIsolationPolicy,
    owner: TransactionId,
    active: impl IntoIterator<Item = TransactionId>,
) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(1),
        policy,
        Some(owner),
        active,
    )
    .unwrap()
}

#[test]
fn read_committed_prevents_dirty_read_but_allows_non_repeatable_read_label() {
    let writer = tx(101);
    let reader = tx(102);
    let row = MvccRowHeader::open_version(10, writer, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(writer, TransactionStatus::InFlight)
        .unwrap();

    let reader_snapshot = snapshot(20, MvccIsolationPolicy::ReadCommitted, reader, [writer]);

    assert!(
        !row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap(),
        "dirty read: read-committed must not see another in-flight transaction"
    );

    mark_committed(&statuses, writer, 1010);

    assert!(
        row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap(),
        "non-repeatable read label: read-committed may observe a later durable status"
    );
}

#[test]
fn repeatable_read_hides_concurrent_commit_and_preserves_read_your_writes() {
    let writer = tx(201);
    let reader = tx(202);
    let concurrent_row = MvccRowHeader::open_version(10, writer, None).unwrap();
    let own_row = MvccRowHeader::open_version(30, reader, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(writer, TransactionStatus::InFlight)
        .unwrap();
    statuses
        .record(reader, TransactionStatus::InFlight)
        .unwrap();

    let reader_snapshot = snapshot(
        20,
        MvccIsolationPolicy::RepeatableRead,
        reader,
        [writer, reader],
    );

    mark_committed(&statuses, writer, 2010);

    assert!(
        !concurrent_row
            .visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap(),
        "repeatable-read snapshot must hide a writer active at snapshot creation"
    );
    assert!(
        own_row
            .visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap(),
        "read-your-writes: the snapshot owner sees its own in-flight write"
    );
}

#[test]
fn long_reader_pins_retention_frontier_until_snapshot_release() {
    let registry = ActiveSnapshotRegistry::new();
    let old_reader = SnapshotHandle::new(50, tx(301)).unwrap();
    let newer_reader = SnapshotHandle::new(100, tx(302)).unwrap();

    registry.register_snapshot(newer_reader).unwrap();
    registry.register_snapshot(old_reader).unwrap();

    assert_eq!(registry.minimum_visible_timestamp(), 50);
    assert!(registry.is_version_garbageable(49));
    assert!(!registry.is_version_garbageable(50));
    assert!(!registry.is_version_garbageable(99));

    registry.release_snapshot(old_reader).unwrap();

    assert_eq!(registry.minimum_visible_timestamp(), 100);
    assert!(registry.is_version_garbageable(99));
    assert!(!registry.is_version_garbageable(100));

    registry.release_snapshot(newer_reader).unwrap();

    assert_eq!(registry.minimum_visible_timestamp(), u64::MAX);
    assert!(registry.is_version_garbageable(100));
}

#[test]
fn rollback_invisibility_applies_to_creators_and_delete_intents() {
    let creator = tx(401);
    let rolled_back_creator = tx(402);
    let rolled_back_deleter = tx(403);
    let reader = tx(404);
    let committed_row_with_rolled_back_delete = MvccRowHeader::open_version(10, creator, None)
        .unwrap()
        .close_version(40, rolled_back_deleter)
        .unwrap();
    let rolled_back_row = MvccRowHeader::open_version(10, rolled_back_creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    mark_committed(&statuses, creator, 4010);
    mark_rolled_back(&statuses, rolled_back_creator, 4020);
    mark_rolled_back(&statuses, rolled_back_deleter, 4030);

    for policy in [
        MvccIsolationPolicy::ReadCommitted,
        MvccIsolationPolicy::RepeatableRead,
    ] {
        let reader_snapshot = snapshot(50, policy, reader, []);

        assert!(
            !rolled_back_row
                .visible_in_snapshot(&reader_snapshot, &statuses)
                .unwrap(),
            "rolled-back creator must never become visible"
        );
        assert!(
            committed_row_with_rolled_back_delete
                .visible_in_snapshot(&reader_snapshot, &statuses)
                .unwrap(),
            "rolled-back delete intent must not hide the committed prior version"
        );
    }
}

#[test]
fn serializable_commit_label_does_not_upgrade_mvcc_visibility_policy() {
    let writer = tx(501);
    let reader = tx(502);
    let entry = CommitLogEntry::from_durable_wal(
        writer,
        Lsn::new(5010),
        Lsn::new(5010),
        EngineTimestamp::from_unix_millis(5010),
        1,
        IsolationLevel::Serializable,
    )
    .unwrap();
    let row = MvccRowHeader::open_version(10, writer, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record_committed_after_durable_wal(writer, entry.commit_lsn, entry.durable_lsn)
        .unwrap();

    let reader_snapshot = snapshot(20, MvccIsolationPolicy::RepeatableRead, reader, [writer]);

    assert_eq!(entry.isolation_level, IsolationLevel::Serializable);
    assert!(
        !row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap(),
        "serializable metadata is not MVCC serializable anomaly enforcement"
    );
}
