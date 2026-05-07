use andromeda_storage::{RedoRecordDecision, WalRecordKind};

use crate::fixtures::{
    assert_decision, assert_decisions, expect_decision, lsn, manifest_at_lsn1,
    manifest_with_required_wal_start, plan_at_lsn1, plan_at_zero, plan_from_manifest,
    transactional_redo, tx, tx_begin, tx_commit,
};

// CAC-36 Committed transaction starting at redo floor: row insert replays.
#[test]
fn test_cac_36_committed_tx_at_redo_floor_replays_row_insert() {
    let scenario_tx = tx(60);
    let records = vec![
        tx_begin(scenario_tx, 4, Some(3)),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 5, 4, b"row"),
        tx_commit(scenario_tx, 6, 5),
    ];
    let manifest = manifest_with_required_wal_start(lsn(4));
    let plan = plan_from_manifest(&manifest, &records);
    assert_decision(&plan, 5, RedoRecordDecision::Replay);
}

// CAC-37 ZERO redo floor keeps committed row insert in the replay window.
#[test]
fn test_cac_37_zero_redo_floor_replays_committed_row_insert() {
    let scenario_tx = tx(62);
    let records = vec![
        tx_begin(scenario_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 2, 1, b"x"),
        tx_commit(scenario_tx, 3, 2),
    ];
    let plan = plan_at_zero(&records);
    assert_decision(&plan, 2, RedoRecordDecision::Replay);
}

// CAC-38 Redo floor exactly at TxBegin LSN: subsequent RowInsert replays.
#[test]
fn test_cac_38_floor_at_tx_begin_row_insert_replayed() {
    let scenario_tx = tx(70);
    let records = vec![
        tx_begin(scenario_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, scenario_tx, 2, 1, b"r"),
        tx_commit(scenario_tx, 3, 2),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decision(&plan, 2, RedoRecordDecision::Replay);
}

// CAC-39 recovered_transaction_id_floor returns the highest transaction id seen.
#[test]
fn test_cac_39_recovered_transaction_id_floor_is_max() {
    let tx_low = tx(100);
    let tx_high = tx(999);
    let records = vec![
        tx_begin(tx_low, 1, None),
        transactional_redo(WalRecordKind::RowInsert, tx_low, 2, 1, b"lo"),
        tx_commit(tx_low, 3, 2),
        tx_begin(tx_high, 4, Some(3)),
        transactional_redo(WalRecordKind::RowInsert, tx_high, 5, 4, b"hi"),
        tx_commit(tx_high, 6, 5),
    ];
    let plan = plan_from_manifest(&manifest_at_lsn1(), &records);
    assert_eq!(
        plan.recovered_transaction_id_floor(),
        999,
        "floor must equal the highest tx ID seen"
    );
}

// CAC-40 Committed transaction + trailing incomplete transaction.
#[test]
fn test_cac_40_committed_plus_trailing_incomplete() {
    let committed_tx = tx(80);
    let trailing_tx = tx(81);
    let records = vec![
        tx_begin(committed_tx, 1, None),
        transactional_redo(WalRecordKind::RowInsert, committed_tx, 2, 1, b"done"),
        tx_commit(committed_tx, 3, 2),
        tx_begin(trailing_tx, 4, Some(3)),
        transactional_redo(WalRecordKind::RowUpdate, trailing_tx, 5, 4, b"lost"),
    ];
    let plan = plan_at_lsn1(&records);
    assert_decisions(
        &plan,
        &[
            expect_decision(2, RedoRecordDecision::Replay),
            expect_decision(5, RedoRecordDecision::SkipIncompleteTransaction),
        ],
    );
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![lsn(2)],
        "only the committed RowInsert is in the replay set"
    );
    assert!(
        plan.has_incomplete_transactions(),
        "plan must report incomplete transactions"
    );
}
