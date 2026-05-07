use andromeda_core::AndromedaResult;

use crate::{WalRecord, WalRecordKind};

use super::{ReplayContext, ReplayResult};

pub(super) fn replay_tx_begin(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::TxBegin,
    ))
}

pub(super) fn replay_tx_commit(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::TxCommit,
    ))
}

pub(super) fn replay_tx_rollback(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::TxRollback,
    ))
}

pub(super) fn replay_checkpoint_begin(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::CheckpointBegin,
    ))
}

pub(super) fn replay_checkpoint_end(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    ctx.observe_checkpoint_end(record.header.lsn);
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::CheckpointEnd,
    ))
}

pub(super) fn replay_snapshot_begin(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::SnapshotBegin,
    ))
}

pub(super) fn replay_snapshot_end(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::SnapshotEnd,
    ))
}

pub(super) fn replay_security_audit_append(
    _ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    Ok(ReplayResult::skipped(
        record.header.lsn,
        WalRecordKind::SecurityAuditAppend,
    ))
}
