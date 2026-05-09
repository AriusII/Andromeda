use andromeda_recovery::{
    ManifestSwitchRecoveryTrace, ReplayContext, ReplayOutcome, replay_wal_record,
};
use andromeda_storage_heap::HeapRowRedoPayloadV1;
use andromeda_storage_page::{PageId, PageSize};
use andromeda_types::TransactionId;
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

fn record(kind: WalRecordKind, lsn: u64, payload: Vec<u8>) -> WalRecord {
    record_with_tx(kind, lsn, None, payload)
}

fn tx_record(kind: WalRecordKind, lsn: u64, payload: Vec<u8>) -> WalRecord {
    record_with_tx(kind, lsn, Some(TransactionId::new(1)), payload)
}

fn record_with_tx(
    kind: WalRecordKind,
    lsn: u64,
    transaction_id: Option<TransactionId>,
    payload: Vec<u8>,
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        (lsn > 1).then(|| Lsn::new(lsn - 1)),
        transaction_id,
        payload,
    )
    .expect("test WAL record should be valid")
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

fn index_rebuild_payload(operation_tag: u8, index_id: u64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(28);
    payload.extend_from_slice(b"IDXRBV1\0");
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.push(1);
    payload.push(operation_tag);
    payload.extend_from_slice(&4096u16.to_le_bytes());
    payload.extend_from_slice(&index_id.to_le_bytes());
    payload
}

#[test]
fn recovery_handlers_skip_boundaries_and_track_checkpoint_evidence() {
    let mut ctx = ReplayContext::new();
    replay_wal_record(
        &mut ctx,
        &record(WalRecordKind::CheckpointEnd, 40, Vec::new()),
    )
    .expect("checkpoint boundary should replay");

    assert_eq!(ctx.skipped_count, 1);
    assert_eq!(ctx.latest_checkpoint_end_lsn, Some(Lsn::new(40)));
}

#[test]
fn recovery_handlers_apply_manifest_switch_from_durable_payload() {
    let mut ctx = ReplayContext::new();
    ctx.observe_checkpoint_end(Lsn::new(40));
    ctx.known_manifest_crc_by_version.insert(7, 0xAA55_3311);
    let switch = record(
        WalRecordKind::ManifestSwitch,
        77,
        manifest_switch_payload(7, 90, 40, 45, [9; 32], 0xAA55_3311),
    );

    replay_wal_record(&mut ctx, &switch).expect("manifest switch should replay");

    assert_eq!(ctx.applied_count, 1);
    assert_eq!(
        ctx.active_manifest
            .expect("manifest should be applied")
            .snapshot_id,
        90
    );
    assert!(matches!(
        ctx.manifest_switch_traces.last(),
        Some(ManifestSwitchRecoveryTrace::ManifestSwitchApplied {
            manifest_version: 7,
            ..
        })
    ));
}

#[test]
fn recovery_handlers_record_index_rebuild_evidence_for_valid_btree_payload() {
    let mut ctx = ReplayContext::new();
    let btree = tx_record(
        WalRecordKind::BTreeInsert,
        12,
        index_rebuild_payload(3, 900),
    );

    replay_wal_record(&mut ctx, &btree).expect("valid rebuild payload should not hard fail");

    assert_eq!(ctx.skipped_count, 1);
    assert_eq!(ctx.index_rebuild_required.len(), 1);
    assert_eq!(ctx.index_rebuild_required[0].index_id, 900);
    assert_eq!(
        ctx.index_rebuild_required[0].kind,
        WalRecordKind::BTreeInsert
    );
}

#[test]
fn recovery_handlers_apply_hredov1_heap_insert_idempotently() {
    let mut ctx = ReplayContext::new();
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(5),
        PageSize::KiB16,
        2,
        Lsn::ZERO,
        Lsn::new(10),
        b"tuple".to_vec(),
    )
    .expect("heap row redo payload")
    .encode_for_wal_kind(WalRecordKind::RowInsert)
    .expect("encode for row insert");
    let insert = tx_record(WalRecordKind::RowInsert, 10, payload);

    replay_wal_record(&mut ctx, &insert).expect("first replay should apply");
    replay_wal_record(&mut ctx, &insert).expect("duplicate replay should be idempotent");

    let page = ctx
        .heap_redo_page(PageId::new(5))
        .expect("heap page should be recovered");
    assert_eq!(ctx.applied_count, 2);
    assert_eq!(page.page_lsn(), Lsn::new(10));
    assert_eq!(page.read_tuple(2), Some(b"tuple".as_slice()));
}

#[test]
fn recovery_handlers_fail_closed_for_deferred_record_without_promoted_semantics() {
    let mut ctx = ReplayContext::new();
    let mvcc = tx_record(WalRecordKind::MvccVersionCreate, 3, Vec::new());

    let err = replay_wal_record(&mut ctx, &mvcc).expect_err("deferred handler must fail closed");

    assert!(err.message().contains("not promoted"));
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(
        ctx.error_records[0].outcome,
        ReplayOutcome::NotYetImplemented
    );
}
