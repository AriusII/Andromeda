use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{WalRecord, WalRecordKind};

mod boundary;
mod context;
mod deferred;
mod driver;
mod heap_redo;
mod manifest_switch;
mod outcome;
mod report;
mod target;
mod validation;

use boundary::*;
pub use context::ReplayContext;
use deferred::*;
pub use driver::execute_redo_plan_with_adapter;
pub use heap_redo::{HeapRedoPageState, HeapRedoSlotState};
use manifest_switch::replay_manifest_switch;
pub use outcome::{ReplayOutcome, ReplayResult};
pub use report::{IndexRebuildRequiredEvidence, ManifestSwitchRecoveryTrace, WalReplayReport};
pub use target::{RecoveryReplayTarget, RecoveryWalReplayAdapter};

/// Replay a single durable WAL record against the recovery-owned replay context.
///
/// Storage integration remains responsible for validating whether the recovered
/// context can be published into storage-owned state. The handlers here only
/// apply runtime-independent durable WAL semantics.
pub fn replay_wal_record_result(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    let result = match record.header.kind {
        WalRecordKind::TxBegin => replay_tx_begin(ctx, record)?,
        WalRecordKind::TxCommit => replay_tx_commit(ctx, record)?,
        WalRecordKind::TxRollback => replay_tx_rollback(ctx, record)?,
        WalRecordKind::PageAllocate => replay_page_allocate(ctx, record)?,
        WalRecordKind::PageFormat => replay_page_format(ctx, record)?,
        WalRecordKind::RowInsert => replay_row_insert(ctx, record)?,
        WalRecordKind::RowUpdate => replay_row_update(ctx, record)?,
        WalRecordKind::RowDelete => replay_row_delete(ctx, record)?,
        WalRecordKind::IndexInsert => replay_index_insert(ctx, record)?,
        WalRecordKind::IndexDelete => replay_index_delete(ctx, record)?,
        WalRecordKind::MvccVersionCreate => replay_mvcc_version_create(ctx, record)?,
        WalRecordKind::MvccVersionClose => replay_mvcc_version_close(ctx, record)?,
        WalRecordKind::MapDeltaAppend => replay_map_delta_append(ctx, record)?,
        WalRecordKind::CheckpointBegin => replay_checkpoint_begin(ctx, record)?,
        WalRecordKind::CheckpointEnd => replay_checkpoint_end(ctx, record)?,
        WalRecordKind::SnapshotBegin => replay_snapshot_begin(ctx, record)?,
        WalRecordKind::SnapshotEnd => replay_snapshot_end(ctx, record)?,
        WalRecordKind::ManifestSwitch => replay_manifest_switch(ctx, record)?,
        WalRecordKind::CatalogChangeBegin => replay_catalog_change_begin(ctx, record)?,
        WalRecordKind::CatalogChangeApply => replay_catalog_change_apply(ctx, record)?,
        WalRecordKind::CatalogChangeCommit => replay_catalog_change_commit(ctx, record)?,
        WalRecordKind::SecurityAuditAppend => replay_security_audit_append(ctx, record)?,
        WalRecordKind::BTreeInsert => replay_btree_insert(ctx, record)?,
        WalRecordKind::BTreeDelete => replay_btree_delete(ctx, record)?,
        WalRecordKind::BTreeSplit => replay_btree_split(ctx, record)?,
        WalRecordKind::BTreeMerge => replay_btree_merge(ctx, record)?,
    };

    if result.outcome.is_error() {
        let error_msg = result.error.clone();
        ctx.record_result(result.clone());
        if let Some(msg) = error_msg {
            return Err(recovery_error(&msg));
        }
        return Ok(result);
    }

    ctx.record_result(result.clone());
    Ok(result)
}

pub fn replay_wal_record(ctx: &mut ReplayContext, record: &WalRecord) -> AndromedaResult<()> {
    replay_wal_record_result(ctx, record).map(|_| ())
}

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
