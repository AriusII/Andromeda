//! V0 MVCC visibility contracts. Visibility is driven by durable status table
//! evidence plus the selected isolation policy.

use super::fixtures::durable_status_lsn;
use andromeda_core::{CatalogVersion, TransactionId};
use andromeda_tx::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};

#[test]
fn mvcc_v0_hides_inflight_and_rolled_back_creators_until_durable_commit_is_visible() {
    let inflight_creator = TransactionId::new(201);
    let rolled_back_creator = TransactionId::new(202);
    let committed_creator = TransactionId::new(203);
    let inflight_row = MvccRowHeader::open_version(20, inflight_creator, None).unwrap();
    let rolled_back_row = MvccRowHeader::open_version(20, rolled_back_creator, None).unwrap();
    let committed_row = MvccRowHeader::open_version(20, committed_creator, None).unwrap();
    let repeatable_snapshot = Snapshot::with_context(
        30,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(999)),
        [committed_creator],
    )
    .unwrap();
    let statuses = TransactionStatusTable::new();

    statuses
        .record(inflight_creator, TransactionStatus::InFlight)
        .unwrap();
    assert!(
        !inflight_row
            .visible_in_snapshot(&repeatable_snapshot, &statuses)
            .unwrap()
    );

    statuses
        .record_rolled_back_after_durable_wal(
            rolled_back_creator,
            durable_status_lsn(),
            durable_status_lsn(),
        )
        .unwrap();
    assert!(
        !rolled_back_row
            .visible_in_snapshot(&repeatable_snapshot, &statuses)
            .unwrap()
    );

    statuses
        .record_committed_after_durable_wal(
            committed_creator,
            durable_status_lsn(),
            durable_status_lsn(),
        )
        .unwrap();
    assert!(
        !committed_row
            .visible_in_snapshot(&repeatable_snapshot, &statuses)
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
    assert!(
        committed_row
            .visible_in_snapshot(&fresh_snapshot, &statuses)
            .unwrap()
    );
}

#[test]
fn mvcc_v0_ignores_inflight_and_rolled_back_delete_intents() {
    let creator = TransactionId::new(301);
    let inflight_deleter = TransactionId::new(302);
    let rolled_back_deleter = TransactionId::new(303);
    let committed_deleter = TransactionId::new(304);
    let inflight_delete_row = MvccRowHeader::open_version(10, creator, None)
        .unwrap()
        .close_version(40, inflight_deleter)
        .unwrap();
    let rolled_back_delete_row = MvccRowHeader::open_version(10, creator, None)
        .unwrap()
        .close_version(40, rolled_back_deleter)
        .unwrap();
    let committed_delete_row = MvccRowHeader::open_version(10, creator, None)
        .unwrap()
        .close_version(40, committed_deleter)
        .unwrap();
    let snapshot_with_delete_in_flight = Snapshot::with_context(
        50,
        CatalogVersion::new(2),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(999)),
        [committed_deleter],
    )
    .unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record_committed_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    statuses
        .record(inflight_deleter, TransactionStatus::InFlight)
        .unwrap();
    assert!(
        inflight_delete_row
            .visible_in_snapshot(&snapshot_with_delete_in_flight, &statuses)
            .unwrap()
    );

    statuses
        .record_rolled_back_after_durable_wal(
            rolled_back_deleter,
            durable_status_lsn(),
            durable_status_lsn(),
        )
        .unwrap();
    assert!(
        rolled_back_delete_row
            .visible_in_snapshot(&snapshot_with_delete_in_flight, &statuses)
            .unwrap()
    );

    statuses
        .record_committed_after_durable_wal(
            committed_deleter,
            durable_status_lsn(),
            durable_status_lsn(),
        )
        .unwrap();
    assert!(
        committed_delete_row
            .visible_in_snapshot(&snapshot_with_delete_in_flight, &statuses)
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
        !committed_delete_row
            .visible_in_snapshot(&snapshot_after_delete, &statuses)
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

#[test]
fn mvcc_v0_unrecorded_creator_is_invisible_to_other_transactions() {
    let creator = TransactionId::new(501);
    let observer = TransactionId::new(502);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();

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
        "no durable commit evidence => row must remain invisible"
    );
}

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

#[test]
fn mvcc_v0_rolled_back_creator_never_visible() {
    let creator = TransactionId::new(701);
    let observer = TransactionId::new(702);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record_rolled_back_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
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

#[test]
fn mvcc_v0_isolation_policies_differ_on_concurrent_committer() {
    let creator = TransactionId::new(801);
    let observer = TransactionId::new(802);
    let row = MvccRowHeader::open_version(10, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record_committed_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let rc_snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(observer),
        [creator],
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

#[test]
fn mvcc_v0_versions_after_snapshot_are_invisible() {
    let creator = TransactionId::new(901);
    let observer = TransactionId::new(902);
    let row = MvccRowHeader::open_version(100, creator, None).unwrap();
    let statuses = TransactionStatusTable::new();
    statuses
        .record_committed_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let snapshot = Snapshot::with_context(
        50,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(observer),
        Vec::<TransactionId>::new(),
    )
    .unwrap();

    assert!(!row.visible_in_snapshot(&snapshot, &statuses).unwrap());
}

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
        .record_committed_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
        .unwrap();
    statuses
        .record_committed_after_durable_wal(deleter, durable_status_lsn(), durable_status_lsn())
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
