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
    let statuses = TransactionStatusTable::new();

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
    // RepeatableRead so that listing the deleter as active at snapshot
    // creation continues to hide its delete even after durable commit.
    let snapshot_with_delete_in_flight = Snapshot::with_context(
        50,
        CatalogVersion::new(2),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(999)),
        [deleter],
    )
    .unwrap();
    let statuses = TransactionStatusTable::new();
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
        "RR snapshot that still tracks deleter as active must not observe that delete as visible"
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

#[test]
fn mvcc_compatibility_module_reexports_focused_types() {
    let tx_id = TransactionId::new(401);
    let row = andromeda_tx::mvcc::MvccRowHeader::open_version(10, tx_id, None).unwrap();
    let snapshot = andromeda_tx::mvcc::Snapshot::with_context(
        10,
        CatalogVersion::new(3),
        andromeda_tx::mvcc::MvccIsolationPolicy::ReadCommitted,
        Some(tx_id),
        [tx_id],
    )
    .unwrap();
    let statuses = andromeda_tx::mvcc::TransactionStatusTable::new();
    statuses
        .record(tx_id, andromeda_tx::mvcc::TransactionStatus::InFlight)
        .unwrap();

    assert!(row.visible_in_snapshot(&snapshot, &statuses).unwrap());
}

// ---------------------------------------------------------------------------
// V0 doctrine: visible commit ≡ durable WAL evidence in the status table.
// ---------------------------------------------------------------------------

/// A version whose creator has *no* status entry must be invisible to every
/// other transaction, regardless of how old `begin_ts` is. This locks in
/// the rule that visibility cannot be inferred from timestamp alone.
#[test]
fn mvcc_v0_unrecorded_creator_is_invisible_to_other_transactions() {
    let creator = TransactionId::new(501);
    let observer = TransactionId::new(502);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();

    // Snapshot taken long after `begin_ts`, but creator has no durable
    // commit recorded. Pre-doctrine code would have inferred Committed.
    let snapshot = Snapshot::with_context(
        1_000_000,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(observer),
        Vec::<TransactionId>::new(),
    )
    .unwrap();

    assert!(
        !row.visible_in_snapshot(&snapshot, &statuses).unwrap(),
        "no durable commit evidence ⇒ row must remain invisible"
    );
}

/// The same write *is* visible to its own author (read-your-writes), even
/// while the author is still `InFlight`.
#[test]
fn mvcc_v0_creator_observes_its_own_uncommitted_writes() {
    let creator = TransactionId::new(601);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(creator, TransactionStatus::InFlight)
        .unwrap();

    let snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(creator),
        [creator],
    )
    .unwrap();

    assert!(row.visible_in_snapshot(&snapshot, &statuses).unwrap());
}

/// Rolled-back creators must never be visible, even with a stale snapshot.
#[test]
fn mvcc_v0_rolled_back_creator_never_visible() {
    let creator = TransactionId::new(701);
    let observer = TransactionId::new(702);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(creator, TransactionStatus::RolledBack)
        .unwrap();

    for policy in [
        MvccIsolationPolicy::ReadCommitted,
        MvccIsolationPolicy::RepeatableRead,
    ] {
        let snapshot = Snapshot::with_context(
            999,
            CatalogVersion::new(1),
            policy,
            Some(observer),
            Vec::<TransactionId>::new(),
        )
        .unwrap();
        assert!(!row.visible_in_snapshot(&snapshot, &statuses).unwrap());
    }
}

/// Read-committed and repeatable-read must differ on the "writer was active
/// at snapshot creation, then committed" case.
#[test]
fn mvcc_v0_isolation_policies_differ_on_concurrent_committer() {
    let creator = TransactionId::new(801);
    let observer = TransactionId::new(802);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(creator, TransactionStatus::Committed)
        .unwrap();

    let rc_snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(observer),
        [creator], // listed as concurrently active
    )
    .unwrap();
    assert!(
        row.visible_in_snapshot(&rc_snapshot, &statuses).unwrap(),
        "read-committed ignores active_tx_ids and shows any durable commit"
    );

    let rr_snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(observer),
        [creator],
    )
    .unwrap();
    assert!(
        !row.visible_in_snapshot(&rr_snapshot, &statuses).unwrap(),
        "repeatable-read hides writers that were active when the snapshot was taken"
    );
}

/// Writes that postdate the snapshot timestamp are invisible even when the
/// writer is durably committed (snapshot-time ordering invariant).
#[test]
fn mvcc_v0_versions_after_snapshot_are_invisible() {
    let creator = TransactionId::new(901);
    let observer = TransactionId::new(902);
    let row = MvccRowHeader::open_version(100, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(creator, TransactionStatus::Committed)
        .unwrap();

    let snapshot = Snapshot::with_context(
        50, // snapshot taken before begin_ts=100
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(observer),
        Vec::<TransactionId>::new(),
    )
    .unwrap();

    assert!(!row.visible_in_snapshot(&snapshot, &statuses).unwrap());
}

/// A delete intent issued strictly after the snapshot must not be observed
/// even when durably committed.
#[test]
fn mvcc_v0_delete_after_snapshot_is_not_observed() {
    let creator = TransactionId::new(1001);
    let deleter = TransactionId::new(1002);
    let observer = TransactionId::new(1003);
    let row = MvccRowHeader::open_version(10, creator, None)
        .unwrap()
        .close_version(80, deleter)
        .unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record(creator, TransactionStatus::Committed)
        .unwrap();
    statuses
        .record(deleter, TransactionStatus::Committed)
        .unwrap();

    let snapshot = Snapshot::with_context(
        50,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(observer),
        Vec::<TransactionId>::new(),
    )
    .unwrap();

    assert!(
        row.visible_in_snapshot(&snapshot, &statuses).unwrap(),
        "delete with end_ts > snapshot.timestamp must not be observed"
    );
}

/// `Snapshot::with_context_validated` rejects a snapshot owned by a
/// transaction that has already reached a terminal state.
#[test]
fn snapshot_validation_rejects_terminal_owner() {
    let owner = TransactionId::new(1101);
    let statuses = TransactionStatusTable::new();
    statuses
        .record(owner, TransactionStatus::Committed)
        .unwrap();

    let err = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(owner),
        Vec::<TransactionId>::new(),
        &statuses,
    )
    .unwrap_err();
    assert!(err.message().contains("not in flight"));
}

/// `Snapshot::with_context_validated` rejects a snapshot whose owner is
/// not registered with the manager.
#[test]
fn snapshot_validation_rejects_unregistered_owner() {
    let owner = TransactionId::new(1201);
    let statuses = TransactionStatusTable::new();

    let err = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(owner),
        Vec::<TransactionId>::new(),
        &statuses,
    )
    .unwrap_err();
    assert!(err.message().contains("not registered"));
}

/// `Snapshot::with_context_validated` rejects an `active_tx_ids` list that
/// names an already-terminal transaction.
#[test]
fn snapshot_validation_rejects_terminal_active_member() {
    let owner = TransactionId::new(1301);
    let stale = TransactionId::new(1302);
    let statuses = TransactionStatusTable::new();
    statuses.record(owner, TransactionStatus::InFlight).unwrap();
    statuses
        .record(stale, TransactionStatus::RolledBack)
        .unwrap();

    let err = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [stale],
        &statuses,
    )
    .unwrap_err();
    assert!(err.message().contains("terminal transaction"));
}

/// Happy path: an in-flight owner with an in-flight peer passes validation.
#[test]
fn snapshot_validation_accepts_in_flight_owner_and_peers() {
    let owner = TransactionId::new(1401);
    let peer = TransactionId::new(1402);
    let statuses = TransactionStatusTable::new();
    statuses.record(owner, TransactionStatus::InFlight).unwrap();
    statuses.record(peer, TransactionStatus::InFlight).unwrap();

    let snapshot = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [peer],
        &statuses,
    )
    .unwrap();
    assert!(snapshot.is_transaction_active(peer));
    assert!(snapshot.is_current_transaction(owner));
}

/// End-to-end: a durable commit recorded through `TransactionManager` is
/// the *only* path that makes a row visible to a peer snapshot.
#[test]
fn mvcc_v0_visibility_requires_manager_durable_commit() {
    use andromeda_tx::TransactionManager;

    let manager = TransactionManager::new();
    let writer = manager.begin().unwrap();
    let reader = manager.begin().unwrap();
    let row = MvccRowHeader::open_version(10, writer, None).unwrap();

    // Build a status table that mirrors the manager's view at this point.
    let statuses = TransactionStatusTable::new();
    statuses
        .record(writer, manager.status(writer).unwrap().unwrap())
        .unwrap();
    statuses
        .record(reader, manager.status(reader).unwrap().unwrap())
        .unwrap();

    let reader_snapshot = Snapshot::with_context_validated(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(reader),
        [writer],
        &statuses,
    )
    .unwrap();

    // Writer still in flight ⇒ invisible to reader.
    assert!(
        !row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap()
    );

    // Drive writer through commit; mirror status table.
    manager.request_commit(writer).unwrap();
    manager.commit_durable(writer, 4242).unwrap();
    statuses
        .record(writer, manager.status(writer).unwrap().unwrap())
        .unwrap();
    assert_eq!(
        manager.status(writer).unwrap(),
        Some(TransactionStatus::Committed),
        "manager must mirror Committed only after durable LSN flush"
    );

    // Under ReadCommitted (active list ignored), now visible.
    assert!(
        row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap()
    );
}

/// A transaction rolled back through the manager remains invisible to
/// peers regardless of timestamp ordering.
#[test]
fn mvcc_v0_manager_rollback_keeps_writes_invisible() {
    use andromeda_tx::TransactionManager;

    let manager = TransactionManager::new();
    let writer = manager.begin().unwrap();
    let reader = manager.begin().unwrap();
    let row = MvccRowHeader::open_version(10, writer, None).unwrap();

    manager.request_rollback(writer).unwrap();
    manager.rollback_durable(writer, 7777).unwrap();

    let statuses = TransactionStatusTable::new();
    statuses
        .record(writer, manager.status(writer).unwrap().unwrap())
        .unwrap();
    statuses
        .record(reader, manager.status(reader).unwrap().unwrap())
        .unwrap();

    let reader_snapshot = Snapshot::with_context_validated(
        100,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(reader),
        Vec::<TransactionId>::new(),
        &statuses,
    )
    .unwrap();

    assert!(
        !row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap()
    );
}
