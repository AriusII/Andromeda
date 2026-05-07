//! Crash Recovery Implementation — 40 Scenario Test Suite
//!
//! # Coverage
//!
//! This file implements the complete crash recovery scenario matrix in two groups:
//!
//! ## Crash-Before-Commit (CBF) — tests 01–20
//!
//! These tests verify that incomplete (or explicitly rolled-back) transactions
//! produce the correct `SkipIncompleteTransaction` / `SkipRolledBackTransaction` /
//! `SkipNonRedoRecord` decisions after a simulated crash.
//!
//! ## Crash-After-Commit (CAC) — tests 21–40
//!
//! These tests verify that fully committed or non-transactional redo-relevant
//! records are always given a `Replay` decision and that boundary conditions
//! (redo floor, floor-exact, tx-ID floor) are handled correctly.
//!
//! # Test Design Conventions
//!
//! * Manual `WalRecord::from_parts` construction gives full control over LSN chains.
//! * `required_wal_start_lsn = Lsn::new(1)` anchors to the first record.
//! * Empty WAL scenarios use `required_wal_start_lsn = Lsn::ZERO`.
//! * `StartupMode::SafeStart` is used throughout (most permissive for test isolation).
//! * `manifest_crc` must be non-zero; we use `0xcafe_dead` as the sentinel.

use andromeda_core::TransactionId;
use andromeda_storage::{
    ConceptualRedoPlan, DatabaseManifest, Lsn, RecoveryPlan, RedoRecordDecision, StartupMode,
    WalRecord, WalRecordKind,
};

/// Build a minimal valid manifest anchored at LSN 1.
fn manifest_at_lsn1() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xcafe_dead,
    }
}

/// Build a minimal valid manifest with `required_wal_start_lsn = Lsn::ZERO`
/// for empty-WAL tests.
fn manifest_zero_start() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn: Lsn::ZERO,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xcafe_dead,
    }
}

fn make_plan(manifest: &DatabaseManifest, records: &[WalRecord]) -> ConceptualRedoPlan {
    RecoveryPlan::from_manifest_and_wal(manifest, StartupMode::SafeStart, records)
        .expect("RecoveryPlan::from_manifest_and_wal must succeed")
}

fn decision_for(plan: &ConceptualRedoPlan, lsn: Lsn) -> RedoRecordDecision {
    let decision = plan
        .records
        .iter()
        .find(|record| record.lsn == lsn)
        .map(|record| record.decision);

    assert!(
        decision.is_some(),
        "redo plan must contain decision for LSN {:?}",
        lsn
    );
    decision.expect("redo decision presence asserted")
}

// CBF-01  TxBegin only → SkipNonRedoRecord
#[test]
fn test_cbf_01_tx_begin_only_skipped() {
    let tx = TransactionId::new(1);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(1)),
        RedoRecordDecision::SkipNonRedoRecord
    );
}

// CBF-02  TxBegin + RowInsert (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_02_row_insert_no_commit() {
    let tx = TransactionId::new(2);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"row".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-03  TxBegin + RowUpdate (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_03_row_update_no_commit() {
    let tx = TransactionId::new(3);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"upd".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-04  TxBegin + RowDelete (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_04_row_delete_no_commit() {
    let tx = TransactionId::new(4);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowDelete,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"del".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-05  TxBegin + IndexInsert (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_05_index_insert_no_commit() {
    let tx = TransactionId::new(5);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::IndexInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"idx".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-06  TxBegin + IndexDelete (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_06_index_delete_no_commit() {
    let tx = TransactionId::new(6);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::IndexDelete,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"idel".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-07  TxBegin + MvccVersionCreate (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_07_mvcc_version_create_no_commit() {
    let tx = TransactionId::new(7);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::MvccVersionCreate,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"ver".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-08  TxBegin + MvccVersionClose (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_08_mvcc_version_close_no_commit() {
    let tx = TransactionId::new(8);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::MvccVersionClose,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"clo".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-09  TxBegin + CatalogChangeApply (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_09_catalog_change_apply_no_commit() {
    let tx = TransactionId::new(9);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::CatalogChangeApply,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"cat".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-10  Multiple RowInserts in same incomplete tx → all SkipIncomplete
#[test]
fn test_cbf_10_multiple_row_inserts_incomplete() {
    let tx = TransactionId::new(10);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"a".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            b"b".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tx),
            b"c".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    for lsn in [Lsn::new(2), Lsn::new(3), Lsn::new(4)] {
        assert_eq!(
            decision_for(&plan, lsn),
            RedoRecordDecision::SkipIncompleteTransaction,
            "expected SkipIncomplete at LSN {:?}",
            lsn
        );
    }
}

// CBF-11  Mixed RowInsert + RowUpdate (incomplete) → both SkipIncomplete
#[test]
fn test_cbf_11_mixed_row_ops_incomplete() {
    let tx = TransactionId::new(11);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"ins".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            b"upd".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
    assert_eq!(
        decision_for(&plan, Lsn::new(3)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-12  Explicit rollback → SkipRolledBackTransaction
#[test]
fn test_cbf_12_explicit_rollback() {
    let tx = TransactionId::new(12);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"data".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxRollback,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipRolledBackTransaction
    );
}

// CBF-13  TxBegin + TxRollback only → both SkipNonRedoRecord (not redo-relevant)
#[test]
fn test_cbf_13_tx_begin_rollback_both_non_redo() {
    let tx = TransactionId::new(13);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxRollback,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    // Neither TxBegin nor TxRollback is redo-relevant.
    assert_eq!(
        decision_for(&plan, Lsn::new(1)),
        RedoRecordDecision::SkipNonRedoRecord
    );
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipNonRedoRecord
    );
}

// CBF-14  Two independent incomplete transactions → all SkipIncomplete
#[test]
fn test_cbf_14_two_incomplete_transactions() {
    let tx_a = TransactionId::new(14);
    let tx_b = TransactionId::new(15);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx_a),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx_a),
            b"a".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx_b),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tx_b),
            b"b".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
    assert_eq!(
        decision_for(&plan, Lsn::new(4)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
    assert_eq!(plan.incomplete_transactions.len(), 2);
}

// CBF-15  BTreeInsert (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_15_btree_insert_no_commit() {
    let tx = TransactionId::new(20);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::BTreeInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"bt".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-16  BTreeDelete (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_16_btree_delete_no_commit() {
    let tx = TransactionId::new(21);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::BTreeDelete,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"bd".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-17  BTreeSplit (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_17_btree_split_no_commit() {
    let tx = TransactionId::new(22);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::BTreeSplit,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"bs".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-18  BTreeMerge (no commit) → SkipIncompleteTransaction
#[test]
fn test_cbf_18_btree_merge_no_commit() {
    let tx = TransactionId::new(23);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::BTreeMerge,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"bm".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

// CBF-19  Committed tx before crash + incomplete tx → committed=Replay, incomplete=Skip
#[test]
fn test_cbf_19_committed_then_incomplete() {
    let committed_tx = TransactionId::new(30);
    let incomplete_tx = TransactionId::new(31);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(committed_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(committed_tx),
            b"committed".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(committed_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(incomplete_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(5),
            Some(Lsn::new(4)),
            Some(incomplete_tx),
            b"incomplete".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::Replay,
        "committed RowInsert must be Replay"
    );
    assert_eq!(
        decision_for(&plan, Lsn::new(5)),
        RedoRecordDecision::SkipIncompleteTransaction,
        "incomplete RowInsert must be SkipIncomplete"
    );
}

// CBF-20  Empty WAL (required_wal_start_lsn = ZERO) → no records, no plan errors
#[test]
fn test_cbf_20_empty_wal_zero_anchor() {
    let plan = make_plan(&manifest_zero_start(), &[]);
    assert!(
        plan.records.is_empty(),
        "empty WAL must produce zero records"
    );
    assert!(!plan.has_incomplete_transactions());
    assert_eq!(plan.replay_lsns().count(), 0);
}

// CAC-21  Committed RowInsert → Replay
#[test]
fn test_cac_21_committed_row_insert_replayed() {
    let tx = TransactionId::new(41);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"row".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-22  Committed RowUpdate → Replay
#[test]
fn test_cac_22_committed_row_update_replayed() {
    let tx = TransactionId::new(42);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"upd".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-23  Committed RowDelete → Replay
#[test]
fn test_cac_23_committed_row_delete_replayed() {
    let tx = TransactionId::new(43);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowDelete,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"del".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-24  Committed IndexInsert → Replay
#[test]
fn test_cac_24_committed_index_insert_replayed() {
    let tx = TransactionId::new(44);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::IndexInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"idx".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-25  Committed IndexDelete → Replay
#[test]
fn test_cac_25_committed_index_delete_replayed() {
    let tx = TransactionId::new(45);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::IndexDelete,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"idel".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-26  Committed MvccVersionCreate → Replay
#[test]
fn test_cac_26_committed_mvcc_create_replayed() {
    let tx = TransactionId::new(46);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::MvccVersionCreate,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"mc".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-27  Committed MvccVersionClose → Replay
#[test]
fn test_cac_27_committed_mvcc_close_replayed() {
    let tx = TransactionId::new(47);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::MvccVersionClose,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"mv".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-28  Committed CatalogChangeApply → Replay
#[test]
fn test_cac_28_committed_catalog_apply_replayed() {
    let tx = TransactionId::new(48);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::CatalogChangeApply,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"cca".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-29  Committed CatalogChangeCommit → Replay
#[test]
fn test_cac_29_committed_catalog_commit_replayed() {
    let tx = TransactionId::new(49);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::CatalogChangeCommit,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"ccc".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-30  Multiple committed transactions → all redo-relevant records Replay
#[test]
fn test_cac_30_multiple_committed_transactions_all_replay() {
    let tx_a = TransactionId::new(50);
    let tx_b = TransactionId::new(51);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx_a),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx_a),
            b"a".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx_a),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tx_b),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(5),
            Some(Lsn::new(4)),
            Some(tx_b),
            b"b".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(6),
            Some(Lsn::new(5)),
            Some(tx_b),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
    assert_eq!(decision_for(&plan, Lsn::new(5)), RedoRecordDecision::Replay);
    assert_eq!(plan.replay_lsns().count(), 2);
}

// CAC-31  ManifestSwitch (non-transactional) → always Replay
#[test]
fn test_cac_31_manifest_switch_always_replay() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(1),
            None,
            None,
            b"ms".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(1)), RedoRecordDecision::Replay);
}

// CAC-32  PageAllocate (non-transactional) → always Replay
#[test]
fn test_cac_32_page_allocate_always_replay() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::PageAllocate,
            Lsn::new(1),
            None,
            None,
            b"pa".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(1)), RedoRecordDecision::Replay);
}

// CAC-33  PageFormat (non-transactional) → always Replay
#[test]
fn test_cac_33_page_format_always_replay() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::PageFormat,
            Lsn::new(1),
            None,
            None,
            b"pf".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(1)), RedoRecordDecision::Replay);
}

// CAC-34  MapDeltaAppend (non-transactional) → always Replay
#[test]
fn test_cac_34_map_delta_append_always_replay() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::MapDeltaAppend,
            Lsn::new(1),
            None,
            None,
            b"md".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(1)), RedoRecordDecision::Replay);
}

// CAC-35  SecurityAuditAppend (non-transactional) → always Replay
#[test]
fn test_cac_35_security_audit_always_replay() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            b"sa".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(decision_for(&plan, Lsn::new(1)), RedoRecordDecision::Replay);
}

// CAC-36  Committed tx starting at redo floor → redo-relevant record replays
#[test]
fn test_cac_36_committed_tx_at_redo_floor_replays_row_insert() {
    let tx = TransactionId::new(60);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(5),
            Some(Lsn::new(4)),
            Some(tx),
            b"row".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(6),
            Some(Lsn::new(5)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn: Lsn::new(4),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xcafe_dead,
    };
    let plan = make_plan(&manifest, &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(5)),
        RedoRecordDecision::Replay,
        "RowInsert at LSN 5 should be Replay when floor=4"
    );
}

// CAC-37  ZERO redo floor keeps committed row insert in the replay window
#[test]
fn test_cac_37_zero_redo_floor_replays_committed_row_insert() {
    let tx = TransactionId::new(62);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"x".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let zero_manifest = manifest_zero_start();
    let plan = make_plan(&zero_manifest, &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::Replay,
        "ZERO floor must not cause SkipBeforeRedoStart"
    );
}

// CAC-38  Redo floor exactly at TxBegin LSN → subsequent RowInsert replayed
#[test]
fn test_cac_38_floor_at_tx_begin_row_insert_replayed() {
    let tx = TransactionId::new(70);
    // required_wal_start_lsn = 1 (at TxBegin). Coverage: first record = LSN 1.
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            b"r".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    // TxBegin is at the floor; RowInsert is above the floor and in a committed tx.
    assert_eq!(decision_for(&plan, Lsn::new(2)), RedoRecordDecision::Replay);
}

// CAC-39  recovered_transaction_id_floor returns highest tx ID seen
#[test]
fn test_cac_39_recovered_transaction_id_floor_is_max() {
    let tx_low = TransactionId::new(100);
    let tx_high = TransactionId::new(999);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx_low),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx_low),
            b"lo".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx_low),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(tx_high),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(5),
            Some(Lsn::new(4)),
            Some(tx_high),
            b"hi".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(6),
            Some(Lsn::new(5)),
            Some(tx_high),
            Vec::new(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        plan.recovered_transaction_id_floor(),
        tx_high.get(),
        "floor must equal the highest tx ID seen"
    );
}

// CAC-40  Committed tx + trailing incomplete → committed=Replay, trailing=Skip
#[test]
fn test_cac_40_committed_plus_trailing_incomplete() {
    let committed_tx = TransactionId::new(80);
    let trailing_tx = TransactionId::new(81);
    let records = vec![
        // Committed transaction.
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(committed_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(committed_tx),
            b"done".to_vec(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(committed_tx),
            Vec::new(),
        )
        .unwrap(),
        // Trailing incomplete transaction — crash occurred before commit.
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(4),
            Some(Lsn::new(3)),
            Some(trailing_tx),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::RowUpdate,
            Lsn::new(5),
            Some(Lsn::new(4)),
            Some(trailing_tx),
            b"lost".to_vec(),
        )
        .unwrap(),
    ];
    let plan = make_plan(&manifest_at_lsn1(), &records);
    assert_eq!(
        decision_for(&plan, Lsn::new(2)),
        RedoRecordDecision::Replay,
        "committed RowInsert must be Replay"
    );
    assert_eq!(
        decision_for(&plan, Lsn::new(5)),
        RedoRecordDecision::SkipIncompleteTransaction,
        "trailing RowUpdate must be SkipIncomplete"
    );
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)],
        "only the committed RowInsert is in the replay set"
    );
    assert!(
        plan.has_incomplete_transactions(),
        "plan must report incomplete transactions"
    );
}
