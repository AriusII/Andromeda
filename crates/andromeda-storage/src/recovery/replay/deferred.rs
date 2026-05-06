use andromeda_core::AndromedaResult;

use crate::{WalRecord, WalRecordKind};

use super::{ReplayContext, ReplayResult};

fn deferred_handler(record: &WalRecord, kind: WalRecordKind) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::deferred(record.header.lsn, kind))
}

pub(super) fn replay_page_allocate(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-page-allocate): Fail-stop until page inventory payload
    // decoding and idempotent allocation-bit updates are implemented.
    deferred_handler(record, WalRecordKind::PageAllocate)
}

pub(super) fn replay_page_format(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-page-format): Fail-stop until page-format payload parsing
    // and format compatibility checks are wired to page initialization.
    deferred_handler(record, WalRecordKind::PageFormat)
}

pub(super) fn replay_row_insert(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-row-insert): Fail-stop until heap WAL payload decoding
    // and idempotent slot visibility application are implemented.
    deferred_handler(record, WalRecordKind::RowInsert)
}

pub(super) fn replay_row_update(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-row-update): Fail-stop until heap WAL payload decoding
    // and idempotent in-place/versioned row updates are implemented.
    deferred_handler(record, WalRecordKind::RowUpdate)
}

pub(super) fn replay_row_delete(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-row-delete): Fail-stop until heap slot tombstone replay
    // is idempotent and MVCC-consistent for already-applied deletes.
    deferred_handler(record, WalRecordKind::RowDelete)
}

pub(super) fn replay_index_insert(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-index-insert): Fail-stop until index WAL payload decoding
    // and idempotent insertion against recovered index state are implemented.
    deferred_handler(record, WalRecordKind::IndexInsert)
}

pub(super) fn replay_index_delete(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-index-delete): Fail-stop until index WAL payload decoding
    // and idempotent delete semantics across retries are implemented.
    deferred_handler(record, WalRecordKind::IndexDelete)
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
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-insert): Fail-stop until B-Tree insert redo can
    // apply key/value payloads idempotently while preserving tree invariants.
    deferred_handler(record, WalRecordKind::BTreeInsert)
}

pub(super) fn replay_btree_delete(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-delete): Fail-stop until B-Tree delete redo can
    // remove keys idempotently while preserving structural invariants.
    deferred_handler(record, WalRecordKind::BTreeDelete)
}

pub(super) fn replay_btree_split(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-split): Fail-stop until B-Tree split redo can
    // replay parent/sibling linkage updates deterministically.
    deferred_handler(record, WalRecordKind::BTreeSplit)
}

pub(super) fn replay_btree_merge(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    // TECH-DEBT(recovery-btree-merge): Fail-stop until B-Tree merge redo can
    // replay node consolidation and parent updates idempotently.
    deferred_handler(record, WalRecordKind::BTreeMerge)
}
