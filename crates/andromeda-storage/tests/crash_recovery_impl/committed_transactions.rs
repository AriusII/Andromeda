use andromeda_storage::{RedoRecordDecision, WalRecordKind};

use crate::fixtures::{
    assert_decision, assert_decisions, committed_single_redo, expect_decision, plan_at_lsn1,
    transactional_redo, tx, tx_begin, tx_commit,
};

fn assert_committed_redo_replays(kind: WalRecordKind, tx_id: u64, payload: &'static [u8]) {
    let records = committed_single_redo(tx(tx_id), kind, payload);
    let plan = plan_at_lsn1(&records);
    assert_decision(&plan, 2, RedoRecordDecision::Replay);
}

// CAC-21 Committed RowInsert: Replay.
#[test]
fn test_cac_21_committed_row_insert_replayed() {
    assert_committed_redo_replays(WalRecordKind::RowInsert, 41, b"row");
}

// CAC-22 Committed RowUpdate: Replay.
#[test]
fn test_cac_22_committed_row_update_replayed() {
    assert_committed_redo_replays(WalRecordKind::RowUpdate, 42, b"upd");
}

// CAC-23 Committed RowDelete: Replay.
#[test]
fn test_cac_23_committed_row_delete_replayed() {
    assert_committed_redo_replays(WalRecordKind::RowDelete, 43, b"del");
}

// CAC-24 Committed IndexInsert: Replay.
#[test]
fn test_cac_24_committed_index_insert_replayed() {
    assert_committed_redo_replays(WalRecordKind::IndexInsert, 44, b"idx");
}

// CAC-25 Committed IndexDelete: Replay.
#[test]
fn test_cac_25_committed_index_delete_replayed() {
    assert_committed_redo_replays(WalRecordKind::IndexDelete, 45, b"idel");
}

// CAC-26 Committed MvccVersionCreate: Replay.
#[test]
fn test_cac_26_committed_mvcc_create_replayed() {
    assert_committed_redo_replays(WalRecordKind::MvccVersionCreate, 46, b"mc");
}

// CAC-27 Committed MvccVersionClose: Replay.
#[test]
fn test_cac_27_committed_mvcc_close_replayed() {
    assert_committed_redo_replays(WalRecordKind::MvccVersionClose, 47, b"mv");
}

// CAC-28 Committed CatalogChangeApply: Replay.
#[test]
fn test_cac_28_committed_catalog_apply_replayed() {
    assert_committed_redo_replays(WalRecordKind::CatalogChangeApply, 48, b"cca");
}

// CAC-29 Committed CatalogChangeCommit: Replay.
#[test]
fn test_cac_29_committed_catalog_commit_replayed() {
    assert_committed_redo_replays(WalRecordKind::CatalogChangeCommit, 49, b"ccc");
}

// CAC-30 Multiple committed transactions: all redo-relevant records replay.
#[test]
fn test_cac_30_multiple_committed_transactions_all_replay() {
    let tx_a = tx(50);
    let tx_b = tx(51);
    let records = vec![
        tx_begin(tx_a, 1, None),
        transactional_redo(WalRecordKind::RowInsert, tx_a, 2, 1, b"a"),
        tx_commit(tx_a, 3, 2),
        tx_begin(tx_b, 4, Some(3)),
        transactional_redo(WalRecordKind::RowInsert, tx_b, 5, 4, b"b"),
        tx_commit(tx_b, 6, 5),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(2, RedoRecordDecision::Replay),
            expect_decision(5, RedoRecordDecision::Replay),
        ],
    );
    assert_eq!(plan.replay_lsns().count(), 2);
}
