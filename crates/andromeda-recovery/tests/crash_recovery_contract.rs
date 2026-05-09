//! Crash recovery planning contracts.
//!
//! These tests bind durable WAL evidence to redo decisions without depending on
//! the storage compatibility facade.

use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{ConceptualRedoPlan, RecoveryPlan, RedoRecordDecision, StartupMode};
use andromeda_types::TransactionId;
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

const TRANSACTIONAL_REDO_KINDS: [WalRecordKind; 13] = [
    WalRecordKind::RowInsert,
    WalRecordKind::RowUpdate,
    WalRecordKind::RowDelete,
    WalRecordKind::IndexInsert,
    WalRecordKind::IndexDelete,
    WalRecordKind::MvccVersionCreate,
    WalRecordKind::MvccVersionClose,
    WalRecordKind::CatalogChangeApply,
    WalRecordKind::CatalogChangeCommit,
    WalRecordKind::BTreeInsert,
    WalRecordKind::BTreeDelete,
    WalRecordKind::BTreeSplit,
    WalRecordKind::BTreeMerge,
];

const NON_TRANSACTIONAL_REDO_KINDS: [WalRecordKind; 5] = [
    WalRecordKind::PageAllocate,
    WalRecordKind::PageFormat,
    WalRecordKind::MapDeltaAppend,
    WalRecordKind::ManifestSwitch,
    WalRecordKind::SecurityAuditAppend,
];

#[test]
fn incomplete_transaction_redo_is_discarded_for_every_replay_family() {
    for (index, kind) in TRANSACTIONAL_REDO_KINDS.into_iter().enumerate() {
        let tx = TransactionId::new(index as u64 + 1);
        let records = vec![
            tx_begin(tx, 1, None),
            transactional_record(kind, tx, 2, 1, b"incomplete"),
        ];
        let plan = plan_at_lsn1(&records);

        assert_decision(
            &plan,
            Lsn::new(2),
            RedoRecordDecision::SkipIncompleteTransaction,
        );
        assert!(plan.has_incomplete_transactions());
        assert_eq!(plan.incomplete_transactions[0].transaction_id, tx);
    }
}

#[test]
fn rolled_back_transaction_redo_is_discarded_for_every_replay_family() {
    for (index, kind) in TRANSACTIONAL_REDO_KINDS.into_iter().enumerate() {
        let tx = TransactionId::new(index as u64 + 100);
        let records = vec![
            tx_begin(tx, 1, None),
            transactional_record(kind, tx, 2, 1, b"rolled-back"),
            tx_rollback(tx, 3, 2),
        ];
        let plan = plan_at_lsn1(&records);

        assert_decision(
            &plan,
            Lsn::new(2),
            RedoRecordDecision::SkipRolledBackTransaction,
        );
        assert!(!plan.has_incomplete_transactions());
        assert_eq!(plan.replay_lsns().count(), 0);
    }
}

#[test]
fn committed_transaction_redo_replays_for_every_replay_family() {
    for (index, kind) in TRANSACTIONAL_REDO_KINDS.into_iter().enumerate() {
        let tx = TransactionId::new(index as u64 + 200);
        let records = vec![
            tx_begin(tx, 1, None),
            transactional_record(kind, tx, 2, 1, b"committed"),
            tx_commit(tx, 3, 2),
        ];
        let plan = plan_at_lsn1(&records);

        assert_decision(&plan, Lsn::new(2), RedoRecordDecision::Replay);
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(plan.recovered_transaction_id_floor(), tx.get());
    }
}

#[test]
fn non_transactional_redo_replays_without_transaction_evidence() {
    for kind in NON_TRANSACTIONAL_REDO_KINDS {
        let lsn = Lsn::new(1);
        let record = WalRecord::from_parts(kind, lsn, None, None, b"redo")
            .expect("non-transactional redo record should be structurally valid");
        let plan = plan_at_lsn1(&[record]);

        assert_decision(&plan, lsn, RedoRecordDecision::Replay);
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![lsn]);
    }
}

#[test]
fn manifest_redo_floor_replays_committed_transaction_started_at_floor() {
    let tx = TransactionId::new(300);
    let records = vec![
        tx_begin(tx, 4, Some(3)),
        transactional_record(WalRecordKind::RowInsert, tx, 5, 4, b"row"),
        tx_commit(tx, 6, 5),
    ];
    let plan = plan_from_manifest(&manifest_with_required_wal_start(Lsn::new(4)), &records);

    assert_decision(&plan, Lsn::new(5), RedoRecordDecision::Replay);
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(5)]);
}

#[test]
fn bootstrap_zero_redo_floor_keeps_committed_first_epoch_records() {
    let tx = TransactionId::new(301);
    let records = vec![
        tx_begin(tx, 1, None),
        transactional_record(WalRecordKind::RowInsert, tx, 2, 1, b"bootstrap"),
        tx_commit(tx, 3, 2),
    ];
    let plan = plan_from_manifest(&manifest_with_required_wal_start(Lsn::ZERO), &records);

    assert_decision(&plan, Lsn::new(2), RedoRecordDecision::Replay);
    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
}

#[test]
fn recovered_transaction_id_floor_is_highest_durable_transaction_id() {
    let low = TransactionId::new(401);
    let high = TransactionId::new(999);
    let records = vec![
        tx_begin(low, 1, None),
        transactional_record(WalRecordKind::RowInsert, low, 2, 1, b"low"),
        tx_commit(low, 3, 2),
        tx_begin(high, 4, Some(3)),
        transactional_record(WalRecordKind::RowUpdate, high, 5, 4, b"high"),
        tx_commit(high, 6, 5),
    ];
    let plan = plan_at_lsn1(&records);

    assert_eq!(plan.recovered_transaction_id_floor(), high.get());
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2), Lsn::new(5)]
    );
}

#[test]
fn committed_prefix_with_trailing_incomplete_transaction_replays_only_committed_redo() {
    let committed = TransactionId::new(501);
    let trailing = TransactionId::new(502);
    let records = vec![
        tx_begin(committed, 1, None),
        transactional_record(WalRecordKind::RowInsert, committed, 2, 1, b"committed"),
        tx_commit(committed, 3, 2),
        tx_begin(trailing, 4, Some(3)),
        transactional_record(WalRecordKind::RowUpdate, trailing, 5, 4, b"tail"),
    ];
    let plan = plan_at_lsn1(&records);

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_decision(&plan, Lsn::new(2), RedoRecordDecision::Replay);
    assert_decision(
        &plan,
        Lsn::new(5),
        RedoRecordDecision::SkipIncompleteTransaction,
    );
    assert!(plan.has_incomplete_transactions());
    assert_eq!(plan.incomplete_transactions[0].transaction_id, trailing);
}

fn manifest_with_required_wal_start(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xCAFE_BABE,
    }
}

fn plan_at_lsn1(records: &[WalRecord]) -> ConceptualRedoPlan {
    plan_from_manifest(&manifest_with_required_wal_start(Lsn::new(1)), records)
}

fn plan_from_manifest(manifest: &DatabaseManifest, records: &[WalRecord]) -> ConceptualRedoPlan {
    RecoveryPlan::from_manifest_and_wal(manifest, StartupMode::SafeStart, records)
        .expect("durable WAL fixture must produce a recovery plan")
}

fn assert_decision(plan: &ConceptualRedoPlan, lsn: Lsn, expected: RedoRecordDecision) {
    let decision = plan
        .records
        .iter()
        .find(|record| record.lsn == lsn)
        .map(|record| record.decision);

    assert_eq!(decision, Some(expected), "unexpected decision at {lsn:?}");
}

fn tx_begin(tx: TransactionId, lsn: u64, previous_lsn: Option<u64>) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxBegin,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(tx),
        [],
    )
    .expect("TxBegin fixture should be valid")
}

fn tx_commit(tx: TransactionId, lsn: u64, previous_lsn: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxCommit,
        Lsn::new(lsn),
        Some(Lsn::new(previous_lsn)),
        Some(tx),
        [],
    )
    .expect("TxCommit fixture should be valid")
}

fn tx_rollback(tx: TransactionId, lsn: u64, previous_lsn: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxRollback,
        Lsn::new(lsn),
        Some(Lsn::new(previous_lsn)),
        Some(tx),
        [],
    )
    .expect("TxRollback fixture should be valid")
}

fn transactional_record(
    kind: WalRecordKind,
    tx: TransactionId,
    lsn: u64,
    previous_lsn: u64,
    payload: &'static [u8],
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        Some(Lsn::new(previous_lsn)),
        Some(tx),
        payload,
    )
    .expect("transactional WAL fixture should be valid")
}
