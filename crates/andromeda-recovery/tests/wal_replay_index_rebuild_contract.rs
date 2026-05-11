//! Index/B-Tree WAL replay contracts while durable B-Tree redo is not promoted.

use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    RecoveryPlan, RecoveryWalReplayAdapter, RedoRecordDecision, ReplayContext, ReplayOutcome,
    StartupMode, WalReplayReport, execute_redo_plan_with_adapter, replay_wal_record,
};
use andromeda_types::TransactionId;
use andromeda_wal::{InMemoryWal, Lsn, WalRecord, WalRecordKind, wal_record_kind_from_tag};

const INDEX_REBUILD_PAYLOAD_MAGIC: &[u8; 8] = b"IDXRBV1\0";

#[test]
fn index_insert_valid_payload_records_rebuild_required_not_inline_apply() {
    assert_rebuild_required(WalRecordKind::IndexInsert, 1, Lsn::new(10), "IndexInsert");
}

#[test]
fn index_delete_valid_payload_records_rebuild_required_not_inline_apply() {
    assert_rebuild_required(WalRecordKind::IndexDelete, 2, Lsn::new(11), "IndexDelete");
}

#[test]
fn btree_split_valid_payload_records_rebuild_required_not_inline_apply() {
    assert_rebuild_required(WalRecordKind::BTreeSplit, 5, Lsn::new(12), "BTreeSplit");
}

#[test]
fn all_index_btree_valid_payloads_record_rebuild_required_not_inline_apply() {
    for (kind, operation_tag, kind_name, lsn) in [
        (WalRecordKind::IndexInsert, 1, "IndexInsert", Lsn::new(30)),
        (WalRecordKind::IndexDelete, 2, "IndexDelete", Lsn::new(31)),
        (WalRecordKind::BTreeInsert, 3, "BTreeInsert", Lsn::new(32)),
        (WalRecordKind::BTreeDelete, 4, "BTreeDelete", Lsn::new(33)),
        (WalRecordKind::BTreeSplit, 5, "BTreeSplit", Lsn::new(34)),
        (WalRecordKind::BTreeMerge, 6, "BTreeMerge", Lsn::new(35)),
    ] {
        assert_rebuild_required(kind, operation_tag, lsn, kind_name);
    }
}

#[test]
fn committed_btree_payloads_enter_redo_plan_and_surface_rebuild_evidence() {
    for (kind, operation_tag, index_id) in [
        (WalRecordKind::BTreeInsert, 3, 777),
        (WalRecordKind::BTreeDelete, 4, 778),
        (WalRecordKind::BTreeSplit, 5, 779),
        (WalRecordKind::BTreeMerge, 6, 780),
    ] {
        assert_committed_btree_payload_enters_redo_plan(kind, operation_tag, index_id);
    }
}

fn assert_committed_btree_payload_enters_redo_plan(
    kind: WalRecordKind,
    operation_tag: u8,
    index_id: u64,
) {
    let tx = TransactionId::new(101);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin should append");
    let btree_lsn = wal
        .append_payload(
            kind,
            Some(tx),
            index_rebuild_payload(1, 0, 1, 4096, operation_tag, index_id),
        )
        .expect("B-Tree WAL payload should append");
    wal.append_tx_commit(tx).expect("commit should append");
    wal.flush_all().expect("test WAL should flush");

    let records = wal.replay_durable();
    let manifest = manifest_for_replay_from(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("committed B-Tree WAL must produce a recovery plan");
    let btree_plan = plan
        .records
        .iter()
        .find(|record| record.lsn == btree_lsn)
        .expect("B-Tree record should be present in the redo plan");

    assert_eq!(btree_plan.kind, kind);
    assert_eq!(btree_plan.decision, RedoRecordDecision::Replay);

    let mut ctx = ReplayContext::new();
    let report =
        replay_wal_from_lsn_into_context(&manifest, StartupMode::SafeStart, &records, &mut ctx)
            .expect("valid B-Tree rebuild evidence should not apply inline redo");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.handler_skipped_count, 1);
    assert_eq!(report.index_rebuild_required_count, 1);
    assert_eq!(report.index_rebuild_required.len(), 1);
    assert_eq!(report.access_path_rebuild_evidence().len(), 1);
    assert_eq!(report.not_yet_implemented_count, 0);
    assert!(!report.has_replay_errors);
    assert!(report.has_replay_work());
    assert!(report.requires_access_path_rebuild());
    assert!(!report.is_clean_recovery());
    assert_eq!(ctx.index_rebuild_required.len(), 1);

    let evidence = &ctx.index_rebuild_required[0];
    assert_eq!(
        report.access_path_rebuild_evidence().first(),
        Some(evidence)
    );
    assert_eq!(evidence.lsn, btree_lsn);
    assert_eq!(evidence.kind, kind);
    assert_eq!(evidence.transaction_id, Some(tx));
    assert_eq!(evidence.index_id, index_id);
    assert_eq!(evidence.payload_checksum, records[1].header.checksum);
    assert!(
        evidence.reason.contains(&format!("{kind:?}"))
            && evidence.reason.contains("index rebuild required")
            && evidence.reason.contains("instead of applying inline redo"),
        "evidence must explain the promotion gate: {}",
        evidence.reason
    );
}

#[test]
fn committed_btree_malformed_payload_fails_recovery_driver_closed() {
    let tx = TransactionId::new(102);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin should append");
    let btree_lsn = wal
        .append_payload(
            WalRecordKind::BTreeDelete,
            Some(tx),
            b"malformed-btree-payload",
        )
        .expect("structural B-Tree WAL payload should append");
    wal.append_tx_commit(tx).expect("commit should append");
    wal.flush_all().expect("test WAL should flush");

    let records = wal.replay_durable();
    let manifest = manifest_for_replay_from(Lsn::new(1));
    let mut ctx = ReplayContext::new();
    let err =
        replay_wal_from_lsn_into_context(&manifest, StartupMode::SafeStart, &records, &mut ctx)
            .expect_err("malformed committed B-Tree WAL must fail recovery closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("BTreeDelete recovery payload is malformed"),
        "error must identify the committed B-Tree promotion gate payload: {}",
        err.message()
    );
    assert!(ctx.index_rebuild_required.is_empty());
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(ctx.error_records[0].lsn, btree_lsn);
    assert_eq!(ctx.error_records[0].kind, WalRecordKind::BTreeDelete);
    assert_eq!(
        ctx.error_records[0].outcome,
        ReplayOutcome::NotYetImplemented
    );
}

#[test]
fn incomplete_btree_payload_is_discarded_without_rebuild_evidence() {
    let tx = TransactionId::new(103);
    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin should append");
    let btree_lsn = wal
        .append_payload(
            WalRecordKind::BTreeMerge,
            Some(tx),
            index_rebuild_payload(1, 0, 1, 4096, 6, 778),
        )
        .expect("B-Tree WAL payload should append");
    wal.flush_all().expect("test WAL should flush");

    let records = wal.replay_durable();
    let manifest = manifest_for_replay_from(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("incomplete B-Tree WAL should still produce a recovery plan");
    let btree_plan = plan
        .records
        .iter()
        .find(|record| record.lsn == btree_lsn)
        .expect("B-Tree record should be present in the redo plan");

    assert_eq!(
        btree_plan.decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let mut ctx = ReplayContext::new();
    let report =
        replay_wal_from_lsn_into_context(&manifest, StartupMode::SafeStart, &records, &mut ctx)
            .expect("incomplete B-Tree transaction should be discarded before handler replay");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.handler_skipped_count, 0);
    assert_eq!(report.index_rebuild_required_count, 0);
    assert!(report.index_rebuild_required.is_empty());
    assert!(report.access_path_rebuild_evidence().is_empty());
    assert_eq!(report.incomplete_transaction_count, 1);
    assert!(ctx.index_rebuild_required.is_empty());
    assert!(ctx.error_records.is_empty());
}

#[test]
fn malformed_index_payload_fails_closed_with_evidence() {
    let record = WalRecord::from_parts(
        WalRecordKind::IndexInsert,
        Lsn::new(20),
        None,
        Some(TransactionId::new(7)),
        b"not-a-recovery-envelope".to_vec(),
    )
    .expect("record should be structurally valid");
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &record).expect_err("malformed index payload must fail closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, 0);
    assert!(ctx.index_rebuild_required.is_empty());
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(
        ctx.error_records[0].outcome,
        ReplayOutcome::NotYetImplemented
    );
    assert_eq!(ctx.error_records[0].error.as_deref(), Some(err.message()));
    assert!(
        err.message()
            .contains("IndexInsert recovery payload is malformed")
            && err.message().contains("expected 28 bytes"),
        "error must identify malformed index replay payload: {}",
        err.message()
    );
}

#[test]
fn unsupported_btree_recovery_format_fails_closed() {
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeSplit,
        Lsn::new(21),
        None,
        Some(TransactionId::new(8)),
        index_rebuild_payload(99, 0, 1, 4096, 5, 700),
    )
    .expect("record should be structurally valid");
    let mut ctx = ReplayContext::new();

    let err = replay_wal_record(&mut ctx, &record)
        .expect_err("unsupported B-Tree recovery format must fail closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(ctx.index_rebuild_required.is_empty());
    assert_eq!(ctx.error_records.len(), 1);
    assert!(
        err.message().contains("major version 99")
            && err.message().contains("not supported")
            && err.message().contains("BTreeKeyFormat"),
        "error must identify unsupported recovery format: {}",
        err.message()
    );
}

#[test]
fn mismatched_index_operation_tag_fails_closed() {
    let record = WalRecord::from_parts(
        WalRecordKind::IndexDelete,
        Lsn::new(22),
        None,
        Some(TransactionId::new(9)),
        index_rebuild_payload(1, 0, 1, 4096, 1, 701),
    )
    .expect("record should be structurally valid");
    let mut ctx = ReplayContext::new();

    let err =
        replay_wal_record(&mut ctx, &record).expect_err("operation tag mismatch must fail closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(ctx.index_rebuild_required.is_empty());
    assert_eq!(ctx.error_records.len(), 1);
    assert!(
        err.message()
            .contains("operation tag 1 does not match record kind IndexDelete"),
        "error must identify the mismatched handler payload: {}",
        err.message()
    );
}

#[test]
fn unknown_index_operation_tag_fails_closed() {
    assert_recovery_payload_fails_closed(
        WalRecordKind::BTreeMerge,
        index_rebuild_payload(1, 0, 1, 4096, 99, 702),
        "unknown index recovery operation tag 99",
    );
}

#[test]
fn zero_max_key_size_fails_closed() {
    assert_recovery_payload_fails_closed(
        WalRecordKind::BTreeInsert,
        index_rebuild_payload(1, 0, 1, 0, 3, 703),
        "max_key_size must not be zero",
    );
}

#[test]
fn zero_index_id_fails_closed() {
    assert_recovery_payload_fails_closed(
        WalRecordKind::BTreeDelete,
        index_rebuild_payload(1, 0, 1, 4096, 4, 0),
        "index_id must not be zero",
    );
}

#[test]
fn reserved_zero_format_version_fails_closed() {
    assert_recovery_payload_fails_closed(
        WalRecordKind::BTreeSplit,
        index_rebuild_payload(0, 0, 1, 4096, 5, 704),
        "B-Tree key format version 0.0 is reserved",
    );
}

#[test]
fn unknown_wal_record_kind_tag_stays_explicit() {
    assert_eq!(wal_record_kind_from_tag(99), None);
}

fn assert_recovery_payload_fails_closed(
    kind: WalRecordKind,
    payload: Vec<u8>,
    expected_message: &'static str,
) {
    let record = WalRecord::from_parts(
        kind,
        Lsn::new(40),
        None,
        Some(TransactionId::new(55)),
        payload,
    )
    .expect("record should be structurally valid");
    let mut ctx = ReplayContext::new();

    let err = replay_wal_record(&mut ctx, &record)
        .expect_err("malformed index/B-Tree recovery payload must fail closed");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(ctx.index_rebuild_required.is_empty());
    assert_eq!(ctx.error_records.len(), 1);
    assert!(
        err.message().contains(expected_message),
        "error must contain `{expected_message}`: {}",
        err.message()
    );
}

fn assert_rebuild_required(
    kind: WalRecordKind,
    operation_tag: u8,
    lsn: Lsn,
    kind_name: &'static str,
) {
    let tx = TransactionId::new(42);
    let record = WalRecord::from_parts(
        kind,
        lsn,
        None,
        Some(tx),
        index_rebuild_payload(1, 0, 1, 4096, operation_tag, 900),
    )
    .expect("record should be structurally valid");
    let mut ctx = ReplayContext::new();

    replay_wal_record(&mut ctx, &record)
        .expect("valid index/B-Tree payload should be marked rebuild-required");

    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, 1);
    assert!(ctx.error_records.is_empty());
    assert_eq!(ctx.index_rebuild_required.len(), 1);

    let evidence = &ctx.index_rebuild_required[0];
    assert_eq!(evidence.lsn, lsn);
    assert_eq!(evidence.kind, kind);
    assert_eq!(evidence.transaction_id, Some(tx));
    assert_eq!(evidence.index_id, 900);
    assert_eq!(evidence.key_format_major, 1);
    assert_eq!(evidence.key_format_minor, 0);
    assert_eq!(evidence.codec_version, 1);
    assert_eq!(evidence.max_key_size, 4096);
    assert_eq!(evidence.payload_len, 28);
    assert_eq!(evidence.payload_checksum, record.header.checksum);
    assert!(
        evidence.reason.contains(kind_name)
            && evidence.reason.contains("index rebuild required")
            && evidence.reason.contains("instead of applying inline redo"),
        "rebuild evidence must explain why inline redo did not run: {}",
        evidence.reason
    );
}

fn index_rebuild_payload(
    major: u32,
    minor: u32,
    codec_version: u8,
    max_key_size: u16,
    operation_tag: u8,
    index_id: u64,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(28);
    payload.extend_from_slice(INDEX_REBUILD_PAYLOAD_MAGIC);
    payload.extend_from_slice(&major.to_le_bytes());
    payload.extend_from_slice(&minor.to_le_bytes());
    payload.push(codec_version);
    payload.push(operation_tag);
    payload.extend_from_slice(&max_key_size.to_le_bytes());
    payload.extend_from_slice(&index_id.to_le_bytes());
    payload
}

fn manifest_for_replay_from(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xCAFE_BABE,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    }
}

fn replay_wal_from_lsn_into_context(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    let plan = RecoveryPlan::from_manifest_and_wal(manifest, startup_mode, durable_records)?;
    let mut adapter = DeferredIndexReplayAdapter;
    execute_redo_plan_with_adapter(&plan, durable_records, ctx, &mut adapter)
}

struct DeferredIndexReplayAdapter;

impl RecoveryWalReplayAdapter<ReplayContext> for DeferredIndexReplayAdapter {
    fn replay_record(
        &mut self,
        ctx: &mut ReplayContext,
        record: &WalRecord,
    ) -> AndromedaResult<()> {
        replay_wal_record(ctx, record)
    }

    fn is_explicit_deferred_replay_error(&self, ctx: &ReplayContext, record: &WalRecord) -> bool {
        let Some(result) = ctx.error_records.last() else {
            return false;
        };
        result.lsn == record.header.lsn
            && result.kind == record.header.kind
            && result.outcome == ReplayOutcome::NotYetImplemented
            && result
                .error
                .as_deref()
                .is_some_and(|message| message.contains("not promoted"))
    }
}
