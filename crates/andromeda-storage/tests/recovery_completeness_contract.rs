//! Recovery completeness contract tests.
//!
//! These tests exercise the public recovery replay API. They intentionally avoid
//! metadata-only scenarios so the suite fails only on observable contract
//! regressions.

use andromeda_core::TransactionId;
use andromeda_storage::{
    InMemoryWal, Lsn, RecoveryPlan, RedoRecordDecision, ReplayContext, ReplayOutcome, StartupMode,
    UndoChainsBuilder, UndoOperation, WalRecord, WalRecordKind, replay_wal_from_lsn_into_context,
    replay_wal_record,
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

const SKIPPED_OR_IMPLEMENTED_KINDS: [WalRecordKind; 12] = [
    WalRecordKind::TxBegin,
    WalRecordKind::TxCommit,
    WalRecordKind::TxRollback,
    WalRecordKind::RowInsert,
    WalRecordKind::RowUpdate,
    WalRecordKind::RowDelete,
    WalRecordKind::CheckpointBegin,
    WalRecordKind::CheckpointEnd,
    WalRecordKind::SnapshotBegin,
    WalRecordKind::SnapshotEnd,
    WalRecordKind::ManifestSwitch,
    WalRecordKind::SecurityAuditAppend,
];

const FUTURE_WORK_KINDS: [WalRecordKind; 14] = [
    WalRecordKind::PageAllocate,
    WalRecordKind::PageFormat,
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
    assert_eq!(SKIPPED_OR_IMPLEMENTED_KINDS.len(), 12);
    assert_eq!(FUTURE_WORK_KINDS.len(), 14);

    for kind in &ALL_WAL_RECORD_KINDS {
        let kind_name = format!("{kind:?}");
        let handled = SKIPPED_OR_IMPLEMENTED_KINDS.contains(kind);
        let deferred = FUTURE_WORK_KINDS.contains(kind);
        assert_ne!(
            handled, deferred,
            "{kind_name} must be classified as exactly one recovery category",
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
    for (idx, kind) in skipped_kinds.iter().copied().enumerate() {
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

        if is_index_btree_recovery_kind(kind) {
            assert!(
                message.contains("malformed"),
                "{kind:?} error must identify malformed rebuild evidence payloads"
            );
        } else {
            assert!(
                message.contains("not promoted"),
                "{kind:?} error must identify the recovery promotion gate"
            );
        }
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

fn is_index_btree_recovery_kind(kind: WalRecordKind) -> bool {
    matches!(
        kind,
        WalRecordKind::IndexInsert
            | WalRecordKind::IndexDelete
            | WalRecordKind::BTreeInsert
            | WalRecordKind::BTreeDelete
            | WalRecordKind::BTreeSplit
            | WalRecordKind::BTreeMerge
    )
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
fn manifest_switch_recovery_rejects_non_monotonic_version() {
    let manifest = andromeda_storage::DatabaseManifest {
        database_id: 1,
        manifest_version: 5,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(10),
        required_wal_start_lsn: Lsn::new(20),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xABCD_5505,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::CheckpointEnd,
            Lsn::new(20),
            Some(Lsn::new(19)),
            None,
            Vec::new(),
        )
        .expect("checkpoint record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(21),
            Some(Lsn::new(20)),
            None,
            manifest_switch_payload(5, 11, 20, 20, [1; 32], 0xABCD_5505),
        )
        .expect("manifest switch record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("manifest switch must fail closed when version does not advance");

    assert!(err.message().contains("regresses manifest version"));
}

#[test]
fn manifest_switch_recovery_rejects_non_advancing_checkpoint_lsn() {
    let manifest = andromeda_storage::DatabaseManifest {
        database_id: 1,
        manifest_version: 5,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(20),
        required_wal_start_lsn: Lsn::new(20),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xABCD_5505,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::CheckpointEnd,
            Lsn::new(20),
            Some(Lsn::new(19)),
            None,
            Vec::new(),
        )
        .expect("checkpoint record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(21),
            Some(Lsn::new(20)),
            None,
            manifest_switch_payload(6, 11, 20, 20, [2; 32], 0xABCD_6606),
        )
        .expect("manifest switch record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("manifest switch must fail closed when checkpoint LSN does not advance");

    assert!(
        err.message()
            .contains("strictly advance base checkpoint LSN")
    );
}

#[test]
fn manifest_switch_recovery_rejects_missing_checkpoint_end_evidence() {
    let manifest = andromeda_storage::DatabaseManifest {
        database_id: 1,
        manifest_version: 5,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(10),
        required_wal_start_lsn: Lsn::new(20),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xABCD_5505,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(20),
            Some(Lsn::new(19)),
            None,
            manifest_switch_payload(6, 11, 20, 20, [3; 32], 0xABCD_6606),
        )
        .expect("manifest switch record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("manifest switch must fail closed without durable checkpoint evidence");

    assert!(
        err.message()
            .contains("base_checkpoint_lsn lacks durable checkpoint_end evidence")
    );
}

#[test]
fn zero_redo_boundary_is_allowed_only_for_explicit_bootstrap() {
    let bootstrap = manifest_for_replay_from(Lsn::ZERO);
    let report = replay_wal_from_lsn_into_context(
        &bootstrap,
        StartupMode::SafeStart,
        &[],
        &mut ReplayContext::new(),
    )
    .expect("empty bootstrap recovery should be explicit and valid");
    assert_eq!(report.replay_start_lsn, Lsn::ZERO);
    assert_eq!(report.total_records, 0);

    let first_epoch_records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("first epoch record should be valid"),
    ];
    let report = replay_wal_from_lsn_into_context(
        &bootstrap,
        StartupMode::SafeStart,
        &first_epoch_records,
        &mut ReplayContext::new(),
    )
    .expect("bootstrap recovery may replay a WAL chain that starts at genesis LSN 1");
    assert_eq!(report.total_records, 1);
    assert_eq!(report.handler_skipped_count, 1);
}

#[test]
fn zero_redo_boundary_rejects_non_bootstrap_snapshot_or_missing_wal_prefix() {
    let mut snapshot_manifest = manifest_for_replay_from(Lsn::ZERO);
    snapshot_manifest.base_checkpoint_lsn = Lsn::new(40);

    let err = replay_wal_from_lsn_into_context(
        &snapshot_manifest,
        StartupMode::SafeStart,
        &[],
        &mut ReplayContext::new(),
    )
    .expect_err("non-bootstrap snapshots must not use ZERO as redo boundary");
    assert!(err.message().contains("bootstrap ZERO redo boundary"));

    let bootstrap = manifest_for_replay_from(Lsn::ZERO);
    let truncated_prefix = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(5),
            Some(Lsn::new(4)),
            None,
            Vec::new(),
        )
        .expect("record should be valid enough for boundary validation"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &bootstrap,
        StartupMode::SafeStart,
        &truncated_prefix,
        &mut ReplayContext::new(),
    )
    .expect_err("ZERO boundary must not hide missing durable WAL prefix");

    assert!(err.message().contains("start at LSN 1"));
}

#[test]
fn wal_replay_rejects_duplicate_or_reordered_lsn_before_visibility() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("first record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("duplicate LSN record should be structurally valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest_for_replay_from(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("recovery must reject duplicate LSNs before replay");

    assert!(
        err.message().contains("strictly increasing by LSN"),
        "error must identify the monotonic LSN invariant: {}",
        err.message()
    );
}

#[test]
fn wal_replay_rejects_previous_lsn_chain_mismatch_before_visibility() {
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .expect("first record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::SecurityAuditAppend,
            Lsn::new(2),
            None,
            None,
            Vec::new(),
        )
        .expect("wrong previous LSN record should be structurally valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest_for_replay_from(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("recovery must reject broken previous-LSN chains before replay");

    assert!(
        err.message().contains("previous LSN chain mismatch"),
        "error must identify the WAL chain invariant: {}",
        err.message()
    );
}

#[test]
fn wal_replay_rejects_conflicting_terminal_records_before_visibility() {
    let tx = TransactionId::new(45);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(tx),
            Vec::new(),
        )
        .expect("begin record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx),
            Vec::new(),
        )
        .expect("commit record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::TxRollback,
            Lsn::new(3),
            Some(Lsn::new(2)),
            Some(tx),
            Vec::new(),
        )
        .expect("rollback record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest_for_replay_from(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("conflicting terminal records must fail recovery closed");

    assert!(
        err.message()
            .contains("duplicate or conflicting terminal records"),
        "error must expose conflicting terminal evidence: {}",
        err.message()
    );
    assert!(
        err.message().contains("TxCommit") && err.message().contains("TxRollback"),
        "error must name both terminal record kinds: {}",
        err.message()
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
