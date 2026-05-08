//! Durable B-Tree promotion contract tests.
//!
//! These tests bind logical B-Tree insert/delete/split behavior to WAL replay
//! and crash-recovery evidence without claiming that page-backed durable B-Tree
//! mutation is promoted. Until the durable engine can apply insert/delete/
//! split/merge idempotently from WAL, recovery must either emit explicit
//! access-path rebuild evidence or fail closed.

#![forbid(unsafe_code)]

#[path = "btree_engine/support.rs"]
mod btree_support;

use andromeda_core::{AndromedaError, AndromedaErrorKind, TransactionId};
use andromeda_storage::format_version::FormatVersion;
use andromeda_storage::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig, BTreeIndexNode, BTreeKeyFormatIdentity,
    BTreeOperationType, DatabaseManifest, InMemoryWal, KeyV1FormatValidator, Lsn, PageId,
    RecoveryPlan, RedoRecordDecision, ReplayContext, ReplayOutcome, RowId, StartupMode,
    WalRecordKind, replay_wal_from_lsn, replay_wal_from_lsn_into_context,
};

use btree_support::{
    assert_leaf_keys_strictly_ordered, default_engine, fill_leaf_to_capacity, leaf_node,
};

const INDEX_REBUILD_PAYLOAD_MAGIC: &[u8; 8] = b"IDXRBV1\0";

#[derive(Debug, Clone, Copy)]
struct DurableBTreeOperation {
    logical_operation: BTreeOperationType,
    wal_kind: WalRecordKind,
    operation_tag: u8,
    index_id: u64,
}

fn durable_btree_operations() -> [DurableBTreeOperation; 4] {
    [
        DurableBTreeOperation {
            logical_operation: BTreeOperationType::Insert,
            wal_kind: WalRecordKind::BTreeInsert,
            operation_tag: 3,
            index_id: 1_001,
        },
        DurableBTreeOperation {
            logical_operation: BTreeOperationType::Delete,
            wal_kind: WalRecordKind::BTreeDelete,
            operation_tag: 4,
            index_id: 1_002,
        },
        DurableBTreeOperation {
            logical_operation: BTreeOperationType::Split,
            wal_kind: WalRecordKind::BTreeSplit,
            operation_tag: 5,
            index_id: 1_003,
        },
        DurableBTreeOperation {
            logical_operation: BTreeOperationType::Merge,
            wal_kind: WalRecordKind::BTreeMerge,
            operation_tag: 6,
            index_id: 1_004,
        },
    ]
}

#[test]
fn logical_btree_ops_do_not_satisfy_durable_promotion_contract() {
    const {
        assert!(
            !BTREE_DURABLE_FORMAT_PROMOTED,
            "durable B-Tree promotion cannot be claimed by in-memory logical operations"
        );
    }

    let mut engine = default_engine();
    for key in [40u8, 10, 30, 20, 50] {
        engine
            .insert(&[key], RowId::new(u64::from(key) * 10))
            .expect("logical in-memory insert should remain usable for contract fixtures");
    }
    engine
        .delete(&[30])
        .expect("logical in-memory delete should remain usable for contract fixtures");

    assert_eq!(engine.search(&[30]).expect("deleted key lookup"), None);
    assert_eq!(
        engine
            .range_scan(&[10], &[60])
            .expect("ordered logical range scan")
            .into_iter()
            .map(RowId::get)
            .collect::<Vec<_>>(),
        vec![100, 200, 400, 500],
    );

    let config = BTreeConfig::default();
    let mut full_leaf = leaf_node(900, Some(899));
    fill_leaf_to_capacity(&mut full_leaf, &config);

    let (promoted_key, right_leaf) = full_leaf
        .split(config.branching_factor)
        .expect("in-memory fixture split should exercise logical split invariants");

    assert!(!promoted_key.is_empty());
    assert_leaf_keys_strictly_ordered(&full_leaf);
    assert_leaf_keys_strictly_ordered(&right_leaf);
    assert_eq!(full_leaf.next_sibling_page_id, Some(right_leaf.page_id));

    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);
    for operation in durable_btree_operations() {
        let error = validator
            .validate_operation(operation.logical_operation)
            .expect_err("durable page-backed B-Tree mutation must remain gated");
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert!(
            error.message().contains("not promoted")
                && error.message().contains(operation.logical_operation.name()),
            "operation gate must identify the missing durable mutation contract: {}",
            error.message()
        );
    }

    let mut page_backed_node = BTreeIndexNode::new_leaf(PageId::new(910), PageId::new(900));
    let split_error = page_backed_node
        .split(config.branching_factor)
        .expect_err("page-backed split must fail closed before durable promotion");
    assert_durable_page_gate(&split_error, "page-backed node split");

    let sibling = BTreeIndexNode::new_leaf(PageId::new(911), PageId::new(900));
    let merge_error = page_backed_node
        .merge(&sibling)
        .expect_err("page-backed merge must fail closed before durable promotion");
    assert_durable_page_gate(&merge_error, "page-backed node merge");
}

#[test]
fn committed_insert_delete_split_merge_wal_crash_replay_requires_rebuild_evidence() {
    let tx = TransactionId::new(8_001);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin B-Tree transaction");

    let mut expected = Vec::new();
    for operation in durable_btree_operations() {
        let lsn = wal
            .append_payload(
                operation.wal_kind,
                Some(tx),
                index_rebuild_payload(operation.operation_tag, operation.index_id),
            )
            .expect("append valid B-Tree rebuild envelope");
        expected.push((operation, lsn));
    }

    wal.append_tx_commit(tx).expect("commit B-Tree transaction");
    wal.flush_all()
        .expect("durable WAL prefix should survive crash");

    let durable_records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .expect("committed B-Tree WAL must produce a recovery plan");

    for (operation, lsn) in &expected {
        let planned = plan
            .records
            .iter()
            .find(|record| record.lsn == *lsn)
            .expect("B-Tree WAL record should be present in redo plan");
        assert_eq!(planned.kind, operation.wal_kind);
        assert_eq!(planned.transaction_id, Some(tx));
        assert_eq!(planned.decision, RedoRecordDecision::Replay);
    }

    let mut ctx = ReplayContext::new();
    let report = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &durable_records,
        &mut ctx,
    )
    .expect("valid B-Tree recovery envelopes should produce rebuild evidence");

    assert_eq!(report.total_records, 6);
    assert_eq!(report.applied_count, 0);
    assert_eq!(report.handler_skipped_count, expected.len());
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.not_yet_implemented_count, 0);
    assert_eq!(report.index_rebuild_required_count, expected.len());
    assert_eq!(report.committed_transaction_count, 1);
    assert_eq!(report.incomplete_transaction_count, 0);
    assert!(!report.has_replay_errors);
    assert!(report.has_replay_work());
    assert!(report.requires_access_path_rebuild());
    assert!(!report.is_clean_recovery());
    assert_eq!(ctx.index_rebuild_required, report.index_rebuild_required);
    assert_eq!(ctx.error_records.len(), 0);

    for ((operation, lsn), evidence) in expected
        .iter()
        .zip(report.access_path_rebuild_evidence().iter())
    {
        let record = durable_records
            .iter()
            .find(|record| record.header.lsn == *lsn)
            .expect("evidence LSN must bind to a durable WAL record");

        assert_eq!(evidence.lsn, *lsn);
        assert_eq!(evidence.kind, operation.wal_kind);
        assert_eq!(evidence.transaction_id, Some(tx));
        assert_eq!(evidence.index_id, operation.index_id);
        assert_eq!(evidence.key_format_major, 1);
        assert_eq!(evidence.key_format_minor, 0);
        assert_eq!(evidence.codec_version, 1);
        assert_eq!(evidence.max_key_size, 4096);
        assert_eq!(evidence.payload_len, 28);
        assert_eq!(evidence.payload_checksum, record.header.checksum);
        assert!(
            evidence
                .reason
                .contains(&format!("{:?}", operation.wal_kind))
                && evidence.reason.contains("index rebuild required")
                && evidence.reason.contains("instead of applying inline redo"),
            "rebuild evidence must explain why durable inline B-Tree redo did not run: {}",
            evidence.reason
        );
    }
}

#[test]
fn crash_before_commit_discards_btree_mutation_suite_before_replay_gate() {
    let tx = TransactionId::new(8_002);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin B-Tree transaction");

    let mut expected = Vec::new();
    for operation in durable_btree_operations() {
        let lsn = wal
            .append_payload(
                operation.wal_kind,
                Some(tx),
                index_rebuild_payload(operation.operation_tag, operation.index_id),
            )
            .expect("append valid B-Tree rebuild envelope");
        expected.push((operation, lsn));
    }

    wal.flush_all()
        .expect("crash survivor lacks a durable commit record");

    let durable_records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .expect("incomplete B-Tree WAL should still produce a recovery plan");

    assert!(plan.has_incomplete_transactions());
    for (operation, lsn) in &expected {
        let planned = plan
            .records
            .iter()
            .find(|record| record.lsn == *lsn)
            .expect("B-Tree WAL record should be present in redo plan");
        assert_eq!(planned.kind, operation.wal_kind);
        assert_eq!(
            planned.decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
    }

    let mut ctx = ReplayContext::new();
    let report = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &durable_records,
        &mut ctx,
    )
    .expect("incomplete B-Tree transaction should be discarded before handler replay");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.handler_skipped_count, 0);
    assert_eq!(report.plan_skipped_count, expected.len() + 1);
    assert_eq!(report.not_yet_implemented_count, 0);
    assert_eq!(report.index_rebuild_required_count, 0);
    assert_eq!(report.incomplete_transaction_count, 1);
    assert_eq!(report.committed_transaction_count, 0);
    assert!(report.has_discarded_transactions());
    assert!(!report.has_replay_errors);
    assert!(ctx.index_rebuild_required.is_empty());
    assert!(ctx.error_records.is_empty());
}

#[test]
fn malformed_committed_btree_payload_fails_closed_without_clean_recovery() {
    let tx = TransactionId::new(8_003);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin B-Tree transaction");
    let merge_lsn = wal
        .append_payload(
            WalRecordKind::BTreeMerge,
            Some(tx),
            b"missing-durable-merge-envelope",
        )
        .expect("append structurally valid but malformed B-Tree merge payload");
    wal.append_tx_commit(tx)
        .expect("commit malformed B-Tree transaction");
    wal.flush_all()
        .expect("malformed committed WAL should be durable crash input");

    let durable_records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let mut ctx = ReplayContext::new();

    let error = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &durable_records,
        &mut ctx,
    )
    .expect_err("malformed committed B-Tree WAL must fail recovery closed");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(
        error
            .message()
            .contains("BTreeMerge recovery payload is malformed")
            && error.message().contains("expected 28 bytes"),
        "recovery must identify the missing B-Tree merge envelope: {}",
        error.message()
    );
    assert!(ctx.index_rebuild_required.is_empty());
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(ctx.error_records[0].lsn, merge_lsn);
    assert_eq!(ctx.error_records[0].kind, WalRecordKind::BTreeMerge);
    assert_eq!(
        ctx.error_records[0].outcome,
        ReplayOutcome::NotYetImplemented
    );
    assert_eq!(ctx.error_records[0].error.as_deref(), Some(error.message()));
}

#[test]
fn unflushed_btree_commit_is_not_recoverable_truth() {
    let tx = TransactionId::new(8_004);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin B-Tree transaction");
    for operation in durable_btree_operations() {
        wal.append_payload(
            operation.wal_kind,
            Some(tx),
            index_rebuild_payload(operation.operation_tag, operation.index_id),
        )
        .expect("append valid B-Tree rebuild envelope");
    }
    wal.append_tx_commit(tx)
        .expect("commit appended only to volatile WAL buffer");

    let durable_records = wal.replay_durable();
    assert!(
        durable_records.is_empty(),
        "unflushed B-Tree WAL records must not become crash-recovery truth"
    );

    let manifest_requiring_wal = test_manifest(Lsn::new(1));
    let coverage_error = RecoveryPlan::from_manifest_and_wal(
        &manifest_requiring_wal,
        StartupMode::SafeStart,
        &durable_records,
    )
    .expect_err("missing durable B-Tree WAL coverage must fail closed");
    assert!(
        coverage_error.message().contains("WAL coverage")
            && coverage_error.message().contains("required WAL start LSN"),
        "coverage failure must identify the missing durable WAL prefix: {}",
        coverage_error.message()
    );

    let manifest_without_redo = test_manifest(Lsn::ZERO);
    let clean_report = replay_wal_from_lsn(
        &manifest_without_redo,
        StartupMode::SafeStart,
        &durable_records,
    )
    .expect("empty durable prefix is valid only when no WAL replay is required");

    assert_eq!(clean_report.total_records, 0);
    assert_eq!(clean_report.applied_count, 0);
    assert_eq!(clean_report.index_rebuild_required_count, 0);
    assert_eq!(clean_report.committed_transaction_count, 0);
    assert!(clean_report.is_clean_recovery());
}

fn assert_durable_page_gate(error: &AndromedaError, operation: &'static str) {
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(
        error.message().contains(operation)
            && error.message().contains("not promoted")
            && error.message().contains("WAL payload decoding")
            && error.message().contains("idempotent recovery"),
        "page-backed durable gate should name the missing promotion work: {}",
        error.message()
    );
}

fn index_rebuild_payload(operation_tag: u8, index_id: u64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(28);
    payload.extend_from_slice(INDEX_REBUILD_PAYLOAD_MAGIC);
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.push(1);
    payload.push(operation_tag);
    payload.extend_from_slice(&4096u16.to_le_bytes());
    payload.extend_from_slice(&index_id.to_le_bytes());
    payload
}

fn test_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
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
