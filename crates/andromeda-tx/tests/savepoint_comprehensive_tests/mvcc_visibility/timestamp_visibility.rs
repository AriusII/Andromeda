//! Timestamp-only row visibility contracts for savepoint snapshots.

use super::*;

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
        end_ts: Some(90),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(!row.visible_at(sp_ts));
}

/// TC-MV-0010
/// A row that straddles the savepoint timestamp is visible at the snapshot.
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
