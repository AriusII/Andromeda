use andromeda_core::AndromedaResult;

use crate::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTreeKeyFormatIdentity, BTreeOperationType,
    KeyV1FormatValidator, WalRecord, WalRecordKind, format_version::FormatVersion,
};

use super::heap_redo::replay_heap_row_record;
use super::{IndexRebuildRequiredEvidence, ReplayContext, ReplayResult};

fn deferred_handler(record: &WalRecord, kind: WalRecordKind) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::deferred(record.header.lsn, kind))
}

fn page_record_promotion_gate(
    record: &WalRecord,
    kind: WalRecordKind,
    missing_contract: &'static str,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::error(
        record.header.lsn,
        kind,
        format!(
            "{kind:?} recovery handler is not promoted; {missing_contract}. \
             Recovery must fail closed instead of inferring a durable page WAL payload format; \
             payload decoding and idempotent apply semantics must be implemented before records of this type can replay."
        ),
    ))
}

const INDEX_REBUILD_PAYLOAD_MAGIC: &[u8; 8] = b"IDXRBV1\0";
const INDEX_REBUILD_PAYLOAD_LEN: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IndexRecoveryPayload {
    key_format: BTreeKeyFormatIdentity,
    operation: BTreeOperationType,
    index_id: u64,
}

fn index_rebuild_handler(
    ctx: &mut ReplayContext,
    record: &WalRecord,
    kind: WalRecordKind,
) -> AndromedaResult<ReplayResult> {
    let payload = match parse_index_recovery_payload(record.payload(), kind) {
        Ok(payload) => payload,
        Err(message) => {
            return Ok(ReplayResult::error(
                record.header.lsn,
                kind,
                format!("{kind:?} recovery payload is malformed: {message}"),
            ));
        }
    };

    if let Some(message) = validate_index_recovery_format(payload.key_format) {
        return Ok(ReplayResult::error(record.header.lsn, kind, message));
    }

    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, payload.key_format);
    if let Err(err) = validator.validate_operation(payload.operation)
        && !err.message().contains("not promoted")
    {
        return Ok(ReplayResult::error(record.header.lsn, kind, err.message()));
    }

    let reason = if BTREE_DURABLE_FORMAT_PROMOTED {
        format!(
            "{kind:?} recovery payload validated, but inline durable B-Tree redo is not wired; index rebuild is required before normal writes resume"
        )
    } else {
        format!(
            "{kind:?} recovery payload validated; durable B-Tree mutation format is not promoted, so recovery records index rebuild required instead of applying inline redo"
        )
    };

    ctx.record_index_rebuild_required(IndexRebuildRequiredEvidence {
        lsn: record.header.lsn,
        kind,
        transaction_id: record.header.transaction_id,
        index_id: payload.index_id,
        key_format_major: payload.key_format.major,
        key_format_minor: payload.key_format.minor,
        codec_version: payload.key_format.codec_version,
        max_key_size: payload.key_format.max_key_size,
        payload_len: record.payload().len(),
        payload_checksum: record.header.checksum,
        reason: reason.clone(),
    });

    Ok(ReplayResult::index_rebuild_required(
        record.header.lsn,
        kind,
        reason,
    ))
}

fn parse_index_recovery_payload(
    bytes: &[u8],
    kind: WalRecordKind,
) -> Result<IndexRecoveryPayload, String> {
    if bytes.len() != INDEX_REBUILD_PAYLOAD_LEN {
        return Err(format!(
            "expected {INDEX_REBUILD_PAYLOAD_LEN} bytes, got {}",
            bytes.len()
        ));
    }
    if &bytes[0..8] != INDEX_REBUILD_PAYLOAD_MAGIC {
        return Err("missing IDXRBV1 recovery envelope magic".to_string());
    }

    let major = read_u32(bytes, 8);
    let minor = read_u32(bytes, 12);
    let codec_version = bytes[16];
    let operation_tag = bytes[17];
    let max_key_size = read_u16(bytes, 18);
    let index_id = read_u64(bytes, 20);

    let Some(operation) = operation_from_tag(operation_tag) else {
        return Err(format!(
            "unknown index recovery operation tag {operation_tag}"
        ));
    };
    let expected_tag = operation_tag_for_kind(kind);
    if operation_tag != expected_tag {
        return Err(format!(
            "operation tag {operation_tag} does not match record kind {kind:?}"
        ));
    }
    if major == 0 && minor == 0 {
        return Err("B-Tree format version 0.0 is reserved".to_string());
    }
    if max_key_size == 0 {
        return Err("max_key_size must not be zero".to_string());
    }
    if index_id == 0 {
        return Err("index_id must not be zero".to_string());
    }

    Ok(IndexRecoveryPayload {
        key_format: BTreeKeyFormatIdentity::new(major, minor, codec_version, max_key_size),
        operation,
        index_id,
    })
}

fn validate_index_recovery_format(format: BTreeKeyFormatIdentity) -> Option<String> {
    if format.major != 1 {
        return Some(format!(
            "B-Tree recovery format major version {} is not supported for inline replay or rebuild evidence; format={format}",
            format.major
        ));
    }
    if format.codec_version != 1 {
        return Some(format!(
            "B-Tree recovery codec version {} is not supported for inline replay or rebuild evidence; format={format}",
            format.codec_version
        ));
    }
    None
}

fn operation_tag_for_kind(kind: WalRecordKind) -> u8 {
    match kind {
        WalRecordKind::IndexInsert => 1,
        WalRecordKind::IndexDelete => 2,
        WalRecordKind::BTreeInsert => 3,
        WalRecordKind::BTreeDelete => 4,
        WalRecordKind::BTreeSplit => 5,
        WalRecordKind::BTreeMerge => 6,
        _ => 0,
    }
}

fn operation_from_tag(tag: u8) -> Option<BTreeOperationType> {
    match tag {
        1 | 3 => Some(BTreeOperationType::Insert),
        2 | 4 => Some(BTreeOperationType::Delete),
        5 => Some(BTreeOperationType::Split),
        6 => Some(BTreeOperationType::Merge),
        _ => None,
    }
}

fn read_u16(bytes: &[u8], start: usize) -> u16 {
    let mut array = [0u8; 2];
    array.copy_from_slice(&bytes[start..start + 2]);
    u16::from_le_bytes(array)
}

fn read_u32(bytes: &[u8], start: usize) -> u32 {
    let mut array = [0u8; 4];
    array.copy_from_slice(&bytes[start..start + 4]);
    u32::from_le_bytes(array)
}

fn read_u64(bytes: &[u8], start: usize) -> u64 {
    let mut array = [0u8; 8];
    array.copy_from_slice(&bytes[start..start + 8]);
    u64::from_le_bytes(array)
}

pub(super) fn replay_page_allocate(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-page-allocate): Fail-stop until page inventory payload
    // decoding and idempotent allocation-bit updates are implemented.
    page_record_promotion_gate(
        record,
        WalRecordKind::PageAllocate,
        "no durable PageAllocate payload schema or page-inventory apply target is defined",
    )
}

pub(super) fn replay_page_format(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-page-format): Fail-stop until page-format payload parsing
    // and format compatibility checks are wired to page initialization.
    page_record_promotion_gate(
        record,
        WalRecordKind::PageFormat,
        "no durable PageFormat payload schema or page-initialization apply target is defined",
    )
}

pub(super) fn replay_row_insert(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    replay_heap_row_record(ctx, record, WalRecordKind::RowInsert)
}

pub(super) fn replay_row_update(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    replay_heap_row_record(ctx, record, WalRecordKind::RowUpdate)
}

pub(super) fn replay_row_delete(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    replay_heap_row_record(ctx, record, WalRecordKind::RowDelete)
}

pub(super) fn replay_index_insert(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    index_rebuild_handler(ctx, record, WalRecordKind::IndexInsert)
}

pub(super) fn replay_index_delete(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    index_rebuild_handler(ctx, record, WalRecordKind::IndexDelete)
}

pub(super) fn replay_mvcc_version_create(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-mvcc-create): Fail-stop until MVCC version header replay
    // and transaction-visibility reconstruction are implemented.
    deferred_handler(record, WalRecordKind::MvccVersionCreate)
}

pub(super) fn replay_mvcc_version_close(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-mvcc-close): Fail-stop until MVCC close replay
    // preserves commit visibility and idempotent close semantics.
    deferred_handler(record, WalRecordKind::MvccVersionClose)
}

pub(super) fn replay_map_delta_append(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-map-delta-append): Fail-stop until map delta payloads
    // can be replayed against recovered map structure versions.
    deferred_handler(record, WalRecordKind::MapDeltaAppend)
}

pub(super) fn replay_catalog_change_begin(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-catalog-change-begin): Fail-stop until catalog WAL
    // transactions are coordinated with catalog snapshot replay.
    deferred_handler(record, WalRecordKind::CatalogChangeBegin)
}

pub(super) fn replay_catalog_change_apply(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-catalog-change-apply): Fail-stop until catalog mutation
    // payloads can be applied idempotently to the recovered catalog state.
    deferred_handler(record, WalRecordKind::CatalogChangeApply)
}

pub(super) fn replay_catalog_change_commit(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-catalog-change-commit): Fail-stop until catalog commit
    // records can publish recovered catalog transaction state atomically.
    deferred_handler(record, WalRecordKind::CatalogChangeCommit)
}

pub(super) fn replay_btree_insert(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    index_rebuild_handler(ctx, record, WalRecordKind::BTreeInsert)
}

pub(super) fn replay_btree_delete(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    index_rebuild_handler(ctx, record, WalRecordKind::BTreeDelete)
}

pub(super) fn replay_btree_split(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    index_rebuild_handler(ctx, record, WalRecordKind::BTreeSplit)
}

pub(super) fn replay_btree_merge(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    index_rebuild_handler(ctx, record, WalRecordKind::BTreeMerge)
}
