//! Delete-intent visibility contracts under ReadCommitted snapshots.

use super::*;

/// TC-MV-0026
/// Row delete visibility: committed delete at end_ts <= snapshot.ts makes row invisible under RC.
#[test]
fn mv_committed_delete_at_or_before_snapshot_ts_hides_row_under_rc() {
    let status_table = TransactionStatusTable::new();
    let creator = TransactionId::new(1);
    let deleter = TransactionId::new(2);
    status_table
        .record_committed_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
        .unwrap();
    status_table
        .record_committed_after_durable_wal(deleter, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(15),
        creator_tx_id: creator,
        deleter_tx_id: Some(deleter),
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20);
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
        .record_committed_after_durable_wal(creator, durable_status_lsn(), durable_status_lsn())
        .unwrap();
    status_table
        .record_committed_after_durable_wal(deleter, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(25),
        creator_tx_id: creator,
        deleter_tx_id: Some(deleter),
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(visible, "delete after snapshot ts must keep row visible");
}
