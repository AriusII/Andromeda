//! Index/B-Tree WAL replay contracts while durable B-Tree redo is not promoted.

use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{
    Lsn, ReplayContext, ReplayOutcome, WalRecord, WalRecordKind, replay_wal_record,
    wal_record_kind_from_tag,
};

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
fn unknown_wal_record_kind_tag_stays_explicit() {
    assert_eq!(wal_record_kind_from_tag(99), None);
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
