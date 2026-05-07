use andromeda_storage::{RedoRecordDecision, WalRecordKind};

use crate::fixtures::{
    assert_decision, assert_decisions, expect_decision, incomplete_single_redo, plan_at_lsn1,
    plan_at_zero, transactional_redo, tx, tx_begin, tx_commit, tx_rollback,
};

fn assert_single_redo_skips_incomplete(kind: WalRecordKind, tx_id: u64, payload: &'static [u8]) {
    let records = incomplete_single_redo(tx(tx_id), kind, payload);
    let plan = plan_at_lsn1(&records);
    assert_decision(&plan, 2, RedoRecordDecision::SkipIncompleteTransaction);
}

// CBF-01 TxBegin only: SkipNonRedoRecord.
#[test]
fn test_cbf_01_tx_begin_only_skipped() {
    let scenario_tx = tx(1);
    let records = vec![tx_begin(scenario_tx, 1, None)];
    let plan = plan_at_lsn1(&records);
    assert_decision(&plan, 1, RedoRecordDecision::SkipNonRedoRecord);
}

// CBF-02 TxBegin + RowInsert, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_02_row_insert_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::RowInsert, 2, b"row");
}

// CBF-03 TxBegin + RowUpdate, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_03_row_update_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::RowUpdate, 3, b"upd");
}

// CBF-04 TxBegin + RowDelete, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_04_row_delete_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::RowDelete, 4, b"del");
}

// CBF-05 TxBegin + IndexInsert, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_05_index_insert_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::IndexInsert, 5, b"idx");
}

// CBF-06 TxBegin + IndexDelete, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_06_index_delete_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::IndexDelete, 6, b"idel");
}

// CBF-07 TxBegin + MvccVersionCreate, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_07_mvcc_version_create_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::MvccVersionCreate, 7, b"ver");
}

// CBF-08 TxBegin + MvccVersionClose, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_08_mvcc_version_close_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::MvccVersionClose, 8, b"clo");
}

// CBF-09 TxBegin + CatalogChangeApply, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_09_catalog_change_apply_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::CatalogChangeApply, 9, b"cat");
}

// CBF-10 Multiple RowInserts in same incomplete transaction: all skip.
#[test]
fn test_cbf_10_multiple_row_inserts_incomplete() {
    let scenario_tx = tx(10);
    let records = vec![
        tx_begin(scenario_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 2, 1, b"a"),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 3, 2, b"b"),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 4, 3, b"c"),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(2, RedoRecordDecision::SkipIncompleteTransaction),
            expect_decision(3, RedoRecordDecision::SkipIncompleteTransaction),
            expect_decision(4, RedoRecordDecision::SkipIncompleteTransaction),
        ],
    );
}

// CBF-11 Mixed RowInsert + RowUpdate, incomplete: both skip.
#[test]
fn test_cbf_11_mixed_row_ops_incomplete() {
    let scenario_tx = tx(11);
    let records = vec![
        tx_begin(scenario_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 2, 1, b"ins"),
        transactional_redo(WalRecordKind::RowUpdate, scenario_tx, 3, 2, b"upd"),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(2, RedoRecordDecision::SkipIncompleteTransaction),
            expect_decision(3, RedoRecordDecision::SkipIncompleteTransaction),
        ],
    );
}

// CBF-12 Explicit rollback: SkipRolledBackTransaction.
#[test]
fn test_cbf_12_explicit_rollback() {
    let scenario_tx = tx(12);
    let records = vec![
        tx_begin(scenario_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 2, 1, b"data"),
        tx_rollback(scenario_tx, 3, 2),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decision(&plan, 2, RedoRecordDecision::SkipRolledBackTransaction);
}

// CBF-13 TxBegin + TxRollback only: both are non-redo records.
#[test]
fn test_cbf_13_tx_begin_rollback_both_non_redo() {
    let scenario_tx = tx(13);
    let records = vec![
        tx_begin(scenario_tx, 1, None),
        tx_rollback(scenario_tx, 2, 1),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(1, RedoRecordDecision::SkipNonRedoRecord),
            expect_decision(2, RedoRecordDecision::SkipNonRedoRecord),
        ],
    );
}

// CBF-14 Two independent incomplete transactions: all redo records skip.
#[test]
fn test_cbf_14_two_incomplete_transactions() {
    let tx_a = tx(14);
    let tx_b = tx(15);
    let records = vec![
        tx_begin(tx_a, 1, None),
        transactional_redo(WalRecordKind::RowInsert, tx_a, 2, 1, b"a"),
        tx_begin(tx_b, 3, Some(2)),
        transactional_redo(WalRecordKind::RowInsert, tx_b, 4, 3, b"b"),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(2, RedoRecordDecision::SkipIncompleteTransaction),
            expect_decision(4, RedoRecordDecision::SkipIncompleteTransaction),
        ],
    );
    assert_eq!(plan.incomplete_transactions.len(), 2);
}

// CBF-15 BTreeInsert, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_15_btree_insert_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::BTreeInsert, 20, b"bt");
}

// CBF-16 BTreeDelete, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_16_btree_delete_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::BTreeDelete, 21, b"bd");
}

// CBF-17 BTreeSplit, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_17_btree_split_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::BTreeSplit, 22, b"bs");
}

// CBF-18 BTreeMerge, no commit: SkipIncompleteTransaction.
#[test]
fn test_cbf_18_btree_merge_no_commit() {
    assert_single_redo_skips_incomplete(WalRecordKind::BTreeMerge, 23, b"bm");
}

// CBF-19 Committed transaction before crash + incomplete transaction.
#[test]
fn test_cbf_19_committed_then_incomplete() {
    let committed_tx = tx(30);
    let incomplete_tx = tx(31);
    let records = vec![
        tx_begin(committed_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, committed_tx, 2, 1, b"committed"),
        tx_commit(committed_tx, 3, 2),
        tx_begin(incomplete_tx, 4, Some(3)),
        transactional_redo(WalRecordKind::RowInsert, incomplete_tx, 5, 4, b"incomplete"),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(2, RedoRecordDecision::Replay),
            expect_decision(5, RedoRecordDecision::SkipIncompleteTransaction),
        ],
    );
}

// CBF-20 Empty WAL with ZERO anchor: no records, no plan errors.
#[test]
fn test_cbf_20_empty_wal_zero_anchor() {
    let plan = plan_at_zero(&[]);
    assert!(
        plan.records.is_empty(),
        "empty WAL must produce zero records"
    );
    assert!(!plan.has_incomplete_transactions());
    assert_eq!(plan.replay_lsns().count(), 0);
}
