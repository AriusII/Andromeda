use andromeda_core::{CatalogVersion, TransactionId};
use andromeda_tx::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionState, TransactionStateMachine,
    TransactionStatus, TransactionStatusTable,
};

#[test]
fn v0_commit_visibility_requires_nonzero_durable_lsn() {
    let mut tx = TransactionStateMachine::new(TransactionId::new(101));

    tx.begin().unwrap();
    tx.request_commit().unwrap();
    assert_eq!(tx.state, TransactionState::Committing);
    assert!(!tx.is_visible_committed());

    let zero_lsn = tx
        .publish_visible_commit_after_durable_flush(0)
        .unwrap_err();
    assert!(zero_lsn.message().contains("must not be zero"));
    assert_eq!(tx.state, TransactionState::Committing);
    assert!(!tx.is_visible_committed());

    tx.publish_visible_commit_after_durable_flush(900).unwrap();
    assert_eq!(tx.state, TransactionState::Committed);
    assert_eq!(tx.durable_commit_lsn, Some(900));
    assert!(tx.is_visible_committed());
}

#[test]
fn v0_rollback_completion_requires_nonzero_durable_lsn() {
    let mut tx = TransactionStateMachine::new(TransactionId::new(102));

    tx.begin().unwrap();
    tx.request_rollback().unwrap();
    assert_eq!(tx.state, TransactionState::RollingBack);
    assert!(!tx.is_durable_rolled_back());

    let zero_lsn = tx.complete_rollback_after_durable_flush(0).unwrap_err();
    assert!(zero_lsn.message().contains("must not be zero"));
    assert_eq!(tx.state, TransactionState::RollingBack);
    assert!(!tx.is_durable_rolled_back());

    tx.complete_rollback_after_durable_flush(901).unwrap();
    assert_eq!(tx.state, TransactionState::RolledBack);
    assert_eq!(tx.durable_rollback_lsn, Some(901));
    assert!(tx.is_durable_rolled_back());
}

#[test]
fn mvcc_v0_hides_inflight_and_rolled_back_creators_until_durable_commit_is_visible() {
    let creator = TransactionId::new(201);
    let row = MvccRowHeader::open_version(20, creator, None).unwrap();
    let repeatable_snapshot = Snapshot::with_context(
        30,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(999)),
        [creator],
    )
    .unwrap();
    let mut statuses = TransactionStatusTable::new();

    statuses
        .record(creator, TransactionStatus::InFlight)
        .unwrap();
    assert!(
        !row.visible_in_snapshot(&repeatable_snapshot, &statuses)
            .unwrap()
    );

    statuses
        .record(creator, TransactionStatus::RolledBack)
        .unwrap();
    assert!(
        !row.visible_in_snapshot(&repeatable_snapshot, &statuses)
            .unwrap()
    );

    statuses
        .record(creator, TransactionStatus::Committed)
        .unwrap();
    assert!(
        !row.visible_in_snapshot(&repeatable_snapshot, &statuses)
            .unwrap(),
        "repeatable-read snapshots must not see transactions active at snapshot creation"
    );

    let fresh_snapshot = Snapshot::with_context(
        30,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(TransactionId::new(999)),
        Vec::<TransactionId>::new(),
    )
    .unwrap();
    assert!(row.visible_in_snapshot(&fresh_snapshot, &statuses).unwrap());
}

#[test]
fn mvcc_v0_ignores_inflight_and_rolled_back_delete_intents() {
    let creator = TransactionId::new(301);
    let deleter = TransactionId::new(302);
    let row = MvccRowHeader::open_version(10, creator, None)
        .unwrap()
        .close_version(40, deleter)
        .unwrap();
    let snapshot_with_delete_in_flight = Snapshot::with_context(
        50,
        CatalogVersion::new(2),
        MvccIsolationPolicy::ReadCommitted,
        Some(TransactionId::new(999)),
        [deleter],
    )
    .unwrap();
    let mut statuses = TransactionStatusTable::new();
    statuses
        .record(creator, TransactionStatus::Committed)
        .unwrap();

    statuses
        .record(deleter, TransactionStatus::InFlight)
        .unwrap();
    assert!(
        row.visible_in_snapshot(&snapshot_with_delete_in_flight, &statuses)
            .unwrap()
    );

    statuses
        .record(deleter, TransactionStatus::RolledBack)
        .unwrap();
    assert!(
        row.visible_in_snapshot(&snapshot_with_delete_in_flight, &statuses)
            .unwrap()
    );

    statuses
        .record(deleter, TransactionStatus::Committed)
        .unwrap();
    assert!(
        row.visible_in_snapshot(&snapshot_with_delete_in_flight, &statuses)
            .unwrap(),
        "snapshot that still tracks deleter as active must not observe that delete as visible"
    );

    let snapshot_after_delete = Snapshot::with_context(
        50,
        CatalogVersion::new(2),
        MvccIsolationPolicy::ReadCommitted,
        Some(TransactionId::new(999)),
        Vec::<TransactionId>::new(),
    )
    .unwrap();
    assert!(
        !row.visible_in_snapshot(&snapshot_after_delete, &statuses)
            .unwrap()
    );
}
