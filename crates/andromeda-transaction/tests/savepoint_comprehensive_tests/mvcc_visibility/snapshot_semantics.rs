//! MVCC snapshot and isolation semantics for savepoint visibility.

use super::*;

/// TC-MV-0005
/// Snapshot at savepoint creation time sees all committed rows with begin_ts <= sp_ts.
#[test]
fn mv_savepoint_snapshot_sees_prior_committed_rows() {
    let status_table = TransactionStatusTable::new();
    let writer_tx = TransactionId::new(10);
    status_table
        .record_committed_after_durable_wal(writer_tx, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: writer_tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(20)),
        [],
    )
    .unwrap();

    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        visible,
        "committed row with begin_ts <= snapshot.ts must be visible"
    );
}

/// TC-MV-0006
/// RepeatableRead hides an in-flight concurrent writer's rows.
#[test]
fn mv_rr_inflight_writer_invisible_to_snapshot() {
    let status_table = TransactionStatusTable::new();
    let concurrent_tx = TransactionId::new(5);
    status_table
        .record(concurrent_tx, TransactionStatus::InFlight)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 3,
        end_ts: None,
        creator_tx_id: concurrent_tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let observer_tx = TransactionId::new(9);
    let snapshot = snapshot_rr(observer_tx, 20, [concurrent_tx]);

    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "in-flight writer must not be visible to other tx snapshot"
    );
}

/// TC-MV-0007
/// Read-your-writes: the creating transaction sees its own uncommitted rows.
#[test]
fn mv_creator_sees_own_inflight_writes() {
    let status_table = TransactionStatusTable::new();
    let tx = TransactionId::new(7);
    status_table
        .record(tx, TransactionStatus::InFlight)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rr(tx, 10, []);

    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(visible, "creator must see its own in-flight writes");
}

/// TC-MV-0011
/// Rollback-to-savepoint restores the target timestamp view.
#[test]
fn mv_rolled_back_savepoint_writes_become_invisible() {
    let status_table = TransactionStatusTable::new();
    let tx = TransactionId::new(11);
    status_table
        .record(tx, TransactionStatus::InFlight)
        .unwrap();

    let discarded_row = MvccRowHeader {
        begin_ts: 200,
        end_ts: None,
        creator_tx_id: tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rr(TransactionId::new(99), 100, []);
    let visible = discarded_row
        .visible_in_snapshot(&snapshot, &status_table)
        .unwrap();
    assert!(
        !visible,
        "row created after savepoint ts must not be visible at savepoint snapshot"
    );
}

/// TC-MV-0012
/// Rolled-back transaction's rows are never visible to any snapshot.
#[test]
fn mv_rolled_back_tx_rows_are_never_visible() {
    let status_table = TransactionStatusTable::new();
    let aborted_tx = TransactionId::new(20);
    status_table
        .record_rolled_back_after_durable_wal(
            aborted_tx,
            durable_status_lsn(),
            durable_status_lsn(),
        )
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: aborted_tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(50);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "rolled-back tx rows must be invisible to all snapshots"
    );
}

/// TC-MV-0013
/// Read-committed snapshot does not see in-flight writes from other transactions.
#[test]
fn mv_rc_snapshot_does_not_see_inflight_foreign_writes() {
    let status_table = TransactionStatusTable::new();
    let writer = TransactionId::new(30);
    status_table
        .record(writer, TransactionStatus::InFlight)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: writer,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(50);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "RC snapshot must not see in-flight foreign writes (no dirty reads)"
    );
}

/// TC-MV-0014
/// An unregistered transaction is invisible to foreign snapshots.
#[test]
fn mv_unregistered_tx_treated_as_inflight_and_invisible() {
    let status_table = TransactionStatusTable::new();
    let unregistered = TransactionId::new(99);

    let row = MvccRowHeader {
        begin_ts: 1,
        end_ts: None,
        creator_tx_id: unregistered,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(100);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "unregistered tx must be treated as InFlight and invisible to foreign snapshots"
    );
}

/// TC-MV-0015
/// Two concurrent transactions with separate savepoints see independent row sets.
#[test]
fn mv_concurrent_tx_savepoints_are_isolated() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    let tx_b = TransactionId::new(2);
    status_table
        .record(tx_a, TransactionStatus::InFlight)
        .unwrap();
    status_table
        .record(tx_b, TransactionStatus::InFlight)
        .unwrap();

    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snap_a = snapshot_rr(tx_a, 10, [tx_b]);
    assert!(
        row_a.visible_in_snapshot(&snap_a, &status_table).unwrap(),
        "tx_a must see its own rows"
    );

    let snap_b = snapshot_rr(tx_b, 10, [tx_a]);
    assert!(
        !row_a.visible_in_snapshot(&snap_b, &status_table).unwrap(),
        "tx_b must not see tx_a's in-flight rows"
    );
}

/// TC-MV-0016
/// RR does not see a transaction that was active at snapshot creation.
#[test]
fn mv_rr_does_not_see_post_snapshot_commits() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    let tx_b = TransactionId::new(2);
    status_table
        .record(tx_a, TransactionStatus::InFlight)
        .unwrap();
    status_table
        .record(tx_b, TransactionStatus::InFlight)
        .unwrap();

    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snap_b = snapshot_rr(tx_b, 10, [tx_a]);

    status_table
        .record_committed_after_durable_wal(tx_a, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let visible = row_a.visible_in_snapshot(&snap_b, &status_table).unwrap();
    assert!(
        !visible,
        "RR snapshot must not see tx that was active at snapshot creation, even after commit"
    );
}

/// TC-MV-0017
/// After tx_a commits, a new RC snapshot sees tx_a's rows.
#[test]
fn mv_rc_sees_newly_committed_rows() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    status_table
        .record_committed_after_durable_wal(tx_a, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let rc_snap = snapshot_rc(10);
    let visible = row_a.visible_in_snapshot(&rc_snap, &status_table).unwrap();
    assert!(visible, "RC snapshot must see committed rows");
}
