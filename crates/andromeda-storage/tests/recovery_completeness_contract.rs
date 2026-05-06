//! Recovery completeness contract tests.
//!
//! These tests exercise the public recovery replay API. They intentionally avoid
//! placeholder-only scenarios so the suite fails only on observable contract
//! regressions.

use andromeda_core::TransactionId;
use andromeda_storage::{
    InMemoryWal, Lsn, RecoveryPlan, RedoRecordDecision, ReplayContext, ReplayOutcome, StartupMode,
    UndoChainsBuilder, UndoOperation, WalRecord, WalRecordKind, replay_wal_record,
};

const ALL_WAL_RECORD_KINDS: [WalRecordKind; 26] = [
    WalRecordKind::TxBegin,
    WalRecordKind::TxCommit,
    WalRecordKind::TxRollback,
    WalRecordKind::PageAllocate,
    WalRecordKind::PageFormat,
    WalRecordKind::RowInsert,
    WalRecordKind::RowUpdate,
    WalRecordKind::RowDelete,
    WalRecordKind::IndexInsert,
    WalRecordKind::IndexDelete,
    WalRecordKind::MvccVersionCreate,
    WalRecordKind::MvccVersionClose,
    WalRecordKind::MapDeltaAppend,
    WalRecordKind::CheckpointBegin,
    WalRecordKind::CheckpointEnd,
    WalRecordKind::SnapshotBegin,
    WalRecordKind::SnapshotEnd,
    WalRecordKind::ManifestSwitch,
    WalRecordKind::CatalogChangeBegin,
    WalRecordKind::CatalogChangeApply,
    WalRecordKind::CatalogChangeCommit,
    WalRecordKind::SecurityAuditAppend,
    WalRecordKind::BTreeInsert,
    WalRecordKind::BTreeDelete,
    WalRecordKind::BTreeSplit,
    WalRecordKind::BTreeMerge,
];

const SKIPPED_OR_IMPLEMENTED_KINDS: [WalRecordKind; 9] = [
    WalRecordKind::TxBegin,
    WalRecordKind::TxCommit,
    WalRecordKind::TxRollback,
    WalRecordKind::CheckpointBegin,
    WalRecordKind::CheckpointEnd,
    WalRecordKind::SnapshotBegin,
    WalRecordKind::SnapshotEnd,
    WalRecordKind::ManifestSwitch,
    WalRecordKind::SecurityAuditAppend,
];

const FUTURE_WORK_KINDS: [WalRecordKind; 17] = [
    WalRecordKind::PageAllocate,
    WalRecordKind::PageFormat,
    WalRecordKind::RowInsert,
    WalRecordKind::RowUpdate,
    WalRecordKind::RowDelete,
    WalRecordKind::IndexInsert,
    WalRecordKind::IndexDelete,
    WalRecordKind::MvccVersionCreate,
    WalRecordKind::MvccVersionClose,
    WalRecordKind::MapDeltaAppend,
    WalRecordKind::CatalogChangeBegin,
    WalRecordKind::CatalogChangeApply,
    WalRecordKind::CatalogChangeCommit,
    WalRecordKind::BTreeInsert,
    WalRecordKind::BTreeDelete,
    WalRecordKind::BTreeSplit,
    WalRecordKind::BTreeMerge,
];

#[test]
fn all_record_kinds_are_classified_exactly_once() {
    assert_eq!(ALL_WAL_RECORD_KINDS.len(), 26);
    assert_eq!(SKIPPED_OR_IMPLEMENTED_KINDS.len(), 9);
    assert_eq!(FUTURE_WORK_KINDS.len(), 17);

    for kind in ALL_WAL_RECORD_KINDS {
        let handled = SKIPPED_OR_IMPLEMENTED_KINDS.contains(&kind);
        let deferred = FUTURE_WORK_KINDS.contains(&kind);
        assert_ne!(
            handled, deferred,
            "{kind:?} must be classified as exactly one recovery category",
        );
    }
}

#[test]
fn marker_and_boundary_records_are_skipped_without_errors() {
    let skipped_kinds = [
        WalRecordKind::TxBegin,
        WalRecordKind::TxCommit,
        WalRecordKind::TxRollback,
        WalRecordKind::CheckpointBegin,
        WalRecordKind::CheckpointEnd,
        WalRecordKind::SnapshotBegin,
        WalRecordKind::SnapshotEnd,
        WalRecordKind::SecurityAuditAppend,
    ];

    let mut ctx = ReplayContext::new();
    for (idx, kind) in skipped_kinds.into_iter().enumerate() {
        let record = record_for_kind(kind, Lsn::new(idx as u64 + 1));
        replay_wal_record(&mut ctx, &record).expect("skipped handler should not fail");
    }

    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, skipped_kinds.len());
    assert!(!ctx.has_errors());
    assert_eq!(
        ctx.last_replayed_lsn,
        Some(Lsn::new(skipped_kinds.len() as u64))
    );
}

#[test]
fn future_work_records_fail_stop_with_clear_error_and_context() {
    for (idx, kind) in FUTURE_WORK_KINDS.into_iter().enumerate() {
        let mut ctx = ReplayContext::new();
        let record = record_for_kind(kind, Lsn::new(idx as u64 + 10));
        let err =
            replay_wal_record(&mut ctx, &record).expect_err("future work handlers must fail-stop");
        let message = err.message();

        assert!(
            message.contains("not yet implemented"),
            "{kind:?} error must identify deferred implementation"
        );
        assert!(
            message.contains(&format!("{kind:?}")),
            "{kind:?} error must include record kind"
        );
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_records.len(), 1);
        assert_eq!(ctx.error_records[0].kind, kind);
        assert_eq!(
            ctx.error_records[0].outcome,
            ReplayOutcome::NotYetImplemented
        );
    }
}

#[test]
fn manifest_switch_with_valid_payload_is_applied() {
    let mut ctx = ReplayContext::new();
    ctx.known_manifest_crc_by_version.insert(7, 0xAA55_3311);
    let record = WalRecord::from_parts(
        WalRecordKind::ManifestSwitch,
        Lsn::new(77),
        Some(Lsn::new(76)),
        None,
        manifest_switch_payload(7, 90, 40, 45, [9; 32], 0xAA55_3311),
    )
    .expect("manifest switch record should be valid");

    replay_wal_record(&mut ctx, &record).expect("manifest switch should replay");

    assert_eq!(ctx.applied_count, 1);
    assert_eq!(ctx.skipped_count, 0);
    assert!(!ctx.has_errors());
    assert_eq!(
        ctx.active_manifest
            .expect("manifest switch should install active manifest")
            .snapshot_id,
        90
    );
}

#[test]
fn redo_plan_and_undo_builder_preserve_opposite_lsn_ordering() {
    let tx_id = TransactionId::new(44);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx_id)
        .expect("begin record should append");
    let insert_lsn = wal
        .append_payload(WalRecordKind::RowInsert, Some(tx_id), b"insert")
        .expect("insert record should append");
    let update_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(tx_id), b"update")
        .expect("update record should append");
    let delete_lsn = wal
        .append_payload(WalRecordKind::RowDelete, Some(tx_id), b"delete")
        .expect("delete record should append");
    wal.append_tx_commit(tx_id)
        .expect("commit record should append");
    wal.flush_all().expect("test WAL should flush");

    let durable_records = wal.replay_durable();
    let manifest = manifest_for_replay_from(insert_lsn);
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .expect("complete committed WAL should produce a redo plan");

    let replay_lsns = plan.replay_lsns().collect::<Vec<_>>();
    assert_eq!(replay_lsns, vec![insert_lsn, update_lsn, delete_lsn]);
    assert!(replay_lsns.windows(2).all(|pair| pair[0] < pair[1]));

    let mut undo_builder = UndoChainsBuilder::new();
    for redo_record in plan
        .records
        .iter()
        .filter(|record| record.decision == RedoRecordDecision::Replay)
    {
        undo_builder
            .add_redo_record(
                redo_record.lsn,
                redo_record.kind,
                redo_record
                    .transaction_id
                    .expect("row redo records should carry transaction ids"),
            )
            .expect("redo record should be accepted into undo builder");
    }

    let chains = undo_builder
        .build()
        .expect("undo builder should create a valid undo chain");
    assert_eq!(chains.len(), 1);
    let chain = &chains[0];
    assert_eq!(chain.transaction_id, tx_id);
    assert_eq!(
        chain
            .records
            .iter()
            .map(|record| record.original_redo_lsn)
            .collect::<Vec<_>>(),
        vec![delete_lsn, update_lsn, insert_lsn]
    );
    assert_eq!(
        chain
            .records
            .iter()
            .map(|record| record.operation)
            .collect::<Vec<_>>(),
        vec![
            UndoOperation::UndoRowDelete,
            UndoOperation::UndoRowUpdate,
            UndoOperation::UndoRowInsert,
        ]
    );
    assert!(
        chain
            .records
            .windows(2)
            .all(|pair| pair[0].original_redo_lsn > pair[1].original_redo_lsn)
    );
}

fn record_for_kind(kind: WalRecordKind, lsn: Lsn) -> WalRecord {
    WalRecord::from_parts(
        kind,
        lsn,
        None,
        kind.requires_transaction_id()
            .then_some(TransactionId::new(1)),
        Vec::new(),
    )
    .expect("test WAL record should be valid")
}

fn manifest_for_replay_from(required_wal_start_lsn: Lsn) -> andromeda_storage::DatabaseManifest {
    andromeda_storage::DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xCAFE_BABE,
    }
}

fn manifest_switch_payload(
    manifest_version: u64,
    snapshot_id: u64,
    base_checkpoint_lsn: u64,
    required_wal_start_lsn: u64,
    previous_manifest_hash: [u8; 32],
    manifest_crc: u32,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(68);
    payload.extend_from_slice(&manifest_version.to_le_bytes());
    payload.extend_from_slice(&snapshot_id.to_le_bytes());
    payload.extend_from_slice(&base_checkpoint_lsn.to_le_bytes());
    payload.extend_from_slice(&required_wal_start_lsn.to_le_bytes());
    payload.extend_from_slice(&previous_manifest_hash);
    payload.extend_from_slice(&manifest_crc.to_le_bytes());
    payload
}
