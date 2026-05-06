use super::*;

// TASK 2 — MVCC Visibility Tests

/// TC-MV-0001
/// A row created at ts=10 and not deleted is visible at ts=10 and ts=15.
#[test]
fn mv_open_row_visible_within_its_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert!(row.visible_at(10));
    assert!(row.visible_at(15));
}

/// TC-MV-0002
/// A row created at ts=10 is invisible at ts=9 (snapshot precedes creation).
#[test]
fn mv_open_row_invisible_before_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert!(!row.visible_at(9));
}

/// TC-MV-0003
/// A row deleted at end_ts=20 is visible at ts=19 and invisible at ts=20.
#[test]
fn mv_closed_row_visibility_at_end_ts_boundary() {
    let row = MvccRowHeader {
        begin_ts: 10,
        end_ts: Some(20),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(row.visible_at(19));
    assert!(!row.visible_at(20));
    assert!(!row.visible_at(25));
}

/// TC-MV-0004
/// A savepoint snapshot at ts=15 does not see rows created at ts=16 (future).
#[test]
fn mv_savepoint_snapshot_excludes_future_writes() {
    let sp_ts = 15_u64;
    let row_future = MvccRowHeader::open_version(16, TransactionId::new(3), None).unwrap();
    assert!(!row_future.visible_at(sp_ts));
}

/// TC-MV-0005
/// Snapshot at savepoint creation time sees all committed rows with begin_ts <= sp_ts.
#[test]
fn mv_savepoint_snapshot_sees_prior_committed_rows() {
    let status_table = TransactionStatusTable::new();
    let writer_tx = TransactionId::new(10);
    status_table
        .record(writer_tx, TransactionStatus::Committed)
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
/// RepeatableRead: an in-flight concurrent writer's rows are invisible to
/// another transaction's snapshot even if ts ordering allows it.
#[test]
fn mv_rr_inflight_writer_invisible_to_snapshot() {
    let status_table = TransactionStatusTable::new();
    let concurrent_tx = TransactionId::new(5);
    // concurrent_tx is InFlight — not yet committed.
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

    // Observer snapshot does not own concurrent_tx.
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

/// TC-MV-0008
/// A row inserted after the savepoint timestamp is a phantom at that snapshot.
#[test]
fn mv_phantom_row_inserted_after_savepoint_ts_is_invisible() {
    let sp_ts = 100_u64;
    let row = MvccRowHeader::open_version(101, TransactionId::new(1), None).unwrap();
    assert!(
        !row.visible_at(sp_ts),
        "phantom row (begin_ts > sp_ts) must not be visible at savepoint snapshot"
    );
}

/// TC-MV-0009
/// A row deleted before the savepoint timestamp is invisible at the snapshot.
#[test]
fn mv_row_deleted_before_savepoint_ts_is_invisible() {
    let sp_ts = 100_u64;
    let row = MvccRowHeader {
        begin_ts: 50,
        end_ts: Some(90), // deleted well before sp_ts
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(!row.visible_at(sp_ts));
}

/// TC-MV-0010
/// A row that straddles the savepoint timestamp: begin_ts < sp_ts < end_ts
/// is visible at the savepoint snapshot.
#[test]
fn mv_row_straddling_savepoint_ts_is_visible() {
    let sp_ts = 100_u64;
    let row = MvccRowHeader {
        begin_ts: 80,
        end_ts: Some(120),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(row.visible_at(sp_ts));
}

/// TC-MV-0011
/// After rollback-to-savepoint, writes made inside the discarded scope must
/// not be visible to the owning transaction's subsequent reads at the target ts.
#[test]
fn mv_rolled_back_savepoint_writes_become_invisible() {
    let status_table = TransactionStatusTable::new();
    let tx = TransactionId::new(11);
    status_table
        .record(tx, TransactionStatus::InFlight)
        .unwrap();

    // Row created inside the discarded savepoint scope — after savepoint ts.
    let discarded_row = MvccRowHeader {
        begin_ts: 200, // post-savepoint
        end_ts: None,
        creator_tx_id: tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // Restore an external snapshot at savepoint ts = 100.
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
        .record(aborted_tx, TransactionStatus::RolledBack)
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
/// A transaction that has no recorded status is treated as InFlight (invisible
/// to foreign snapshots — V0 doctrine: visible commit ≡ durable WAL).
#[test]
fn mv_unregistered_tx_treated_as_inflight_and_invisible() {
    let status_table = TransactionStatusTable::new();
    let unregistered = TransactionId::new(99);
    // Deliberately NOT recorded in status_table.

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

    // Row created by tx_a.
    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // tx_a sees its own row.
    let snap_a = snapshot_rr(tx_a, 10, [tx_b]);
    assert!(
        row_a.visible_in_snapshot(&snap_a, &status_table).unwrap(),
        "tx_a must see its own rows"
    );

    // tx_b does NOT see tx_a's uncommitted row.
    let snap_b = snapshot_rr(tx_b, 10, [tx_a]);
    assert!(
        !row_a.visible_in_snapshot(&snap_b, &status_table).unwrap(),
        "tx_b must not see tx_a's in-flight rows"
    );
}

/// TC-MV-0016
/// After tx_a commits, tx_b with RR isolation still does not see tx_a's rows
/// if tx_a was active at snap_b creation time.
#[test]
fn mv_rr_does_not_see_post_snapshot_commits() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    let tx_b = TransactionId::new(2);
    // tx_a is active when tx_b takes its snapshot.
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

    // tx_b's snapshot lists tx_a as active.
    let snap_b = snapshot_rr(tx_b, 10, [tx_a]);

    // Now tx_a commits.
    status_table
        .record(tx_a, TransactionStatus::Committed)
        .unwrap();

    // RR: tx_b's snapshot was taken while tx_a was active, so tx_a remains invisible.
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
        .record(tx_a, TransactionStatus::Committed)
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

/// TC-MV-0018
/// open_version requires non-zero begin_ts and creator_tx_id.
#[test]
fn mv_open_version_rejects_zero_begin_ts() {
    let err = MvccRowHeader::open_version(0, TransactionId::new(1), None).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0019
/// open_version rejects creator_tx_id == 0.
#[test]
fn mv_open_version_rejects_zero_creator_tx_id() {
    let err = MvccRowHeader::open_version(1, TransactionId::new(0), None).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0020
/// close_version requires end_ts > begin_ts.
#[test]
fn mv_close_version_rejects_end_ts_equal_to_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    let err = row.close_version(10, TransactionId::new(2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0021
/// close_version rejects end_ts < begin_ts.
#[test]
fn mv_close_version_rejects_end_ts_before_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    let err = row.close_version(9, TransactionId::new(2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0022
/// A closed row without a deleter_tx_id fails validation.
#[test]
fn mv_closed_row_without_deleter_tx_id_fails_validation() {
    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(10),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: None, // missing!
        previous_version_ptr: None,
        flags: 0,
    };
    let err = row.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0023
/// An open row with a deleter_tx_id fails validation.
#[test]
fn mv_open_row_with_deleter_tx_id_fails_validation() {
    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)), // must not have deleter for open
        previous_version_ptr: None,
        flags: 0,
    };
    let err = row.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0024
/// previous_version_ptr is optional and does not affect visibility.
#[test]
fn mv_previous_version_ptr_does_not_affect_visibility() {
    let with_ptr = MvccRowHeader::open_version(10, TransactionId::new(1), Some(9999)).unwrap();
    let without_ptr = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert_eq!(with_ptr.visible_at(10), without_ptr.visible_at(10));
    assert_eq!(with_ptr.visible_at(5), without_ptr.visible_at(5));
}

/// TC-MV-0025
/// is_open_version and is_closed_version are mutually exclusive.
#[test]
fn mv_open_and_closed_version_are_mutually_exclusive() {
    let open = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert!(open.is_open_version());
    assert!(!open.is_closed_version());

    let closed = open.close_version(20, TransactionId::new(2)).unwrap();
    assert!(closed.is_closed_version());
    assert!(!closed.is_open_version());
}

/// TC-MV-0026
/// Row delete visibility: committed delete at end_ts <= snapshot.ts makes row invisible under RC.
#[test]
fn mv_committed_delete_at_or_before_snapshot_ts_hides_row_under_rc() {
    let status_table = TransactionStatusTable::new();
    let creator = TransactionId::new(1);
    let deleter = TransactionId::new(2);
    status_table
        .record(creator, TransactionStatus::Committed)
        .unwrap();
    status_table
        .record(deleter, TransactionStatus::Committed)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(15),
        creator_tx_id: creator,
        deleter_tx_id: Some(deleter),
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20); // snapshot at ts=20, delete was at 15
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "committed delete before snapshot ts must hide row under RC"
    );
}

/// TC-MV-0027
/// Row delete visibility: committed delete strictly after snapshot.ts keeps row visible.
#[test]
fn mv_committed_delete_after_snapshot_ts_keeps_row_visible() {
    let status_table = TransactionStatusTable::new();
    let creator = TransactionId::new(1);
    let deleter = TransactionId::new(2);
    status_table
        .record(creator, TransactionStatus::Committed)
        .unwrap();
    status_table
        .record(deleter, TransactionStatus::Committed)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(25), // delete after snapshot ts
        creator_tx_id: creator,
        deleter_tx_id: Some(deleter),
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20); // snapshot at ts=20
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(visible, "delete after snapshot ts must keep row visible");
}

/// TC-MV-0028
/// Snapshot validates non-zero timestamp.
#[test]
fn mv_snapshot_rejects_zero_timestamp() {
    let err = Snapshot::with_context(
        0,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0029
/// Snapshot normalizes active_tx_ids (sort + dedup).
#[test]
fn mv_snapshot_normalizes_active_tx_ids() {
    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(1)),
        [
            TransactionId::new(5),
            TransactionId::new(3),
            TransactionId::new(5), // duplicate
        ],
    )
    .unwrap();
    assert_eq!(
        snapshot.active_tx_ids,
        vec![TransactionId::new(3), TransactionId::new(5)]
    );
}

/// TC-MV-0030
/// Snapshot with a terminal transaction in active_tx_ids is rejected by
/// validate_against_statuses.
#[test]
fn mv_snapshot_rejects_terminal_tx_in_active_list() {
    let status_table = TransactionStatusTable::new();
    let committed_tx = TransactionId::new(5);
    status_table
        .record(committed_tx, TransactionStatus::Committed)
        .unwrap();
    // Also register owner as InFlight.
    let owner = TransactionId::new(9);
    status_table
        .record(owner, TransactionStatus::InFlight)
        .unwrap();

    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [committed_tx],
    )
    .unwrap();

    let err = snapshot
        .validate_against_statuses(&status_table)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}
