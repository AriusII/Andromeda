#![allow(dead_code)]

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    ManifestSwitchRecoveryTrace, RecoveryPlan, RecoveryWalReplayAdapter, ReplayContext,
    ReplayOutcome, StartupMode, WalReplayReport, execute_redo_plan_with_adapter, replay_wal_record,
};
use andromeda_types::TransactionId;
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

pub const ALL_WAL_RECORD_KINDS: [WalRecordKind; 26] = [
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

pub const SKIPPED_OR_IMPLEMENTED_KINDS: [WalRecordKind; 12] = [
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

pub const FUTURE_WORK_KINDS: [WalRecordKind; 14] = [
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

pub fn is_index_btree_recovery_kind(kind: WalRecordKind) -> bool {
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

pub fn record_for_kind(kind: WalRecordKind, lsn: Lsn) -> WalRecord {
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

pub fn manifest_for_replay_from(required_wal_start_lsn: Lsn) -> DatabaseManifest {
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

pub fn manifest_switch_payload(
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

pub fn replay_wal_from_lsn_into_context(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    validate_bootstrap_redo_boundary(manifest, durable_records)?;
    validate_conflicting_terminal_records(durable_records)?;
    let plan = RecoveryPlan::from_manifest_and_wal(manifest, startup_mode, durable_records)?;
    if ctx.active_manifest.is_none() {
        ctx.active_manifest = Some(*manifest);
    }
    let mut adapter = DeferredRecoveryReplayAdapter;
    execute_redo_plan_with_adapter(&plan, durable_records, ctx, &mut adapter)
}

struct DeferredRecoveryReplayAdapter;

impl RecoveryWalReplayAdapter<ReplayContext> for DeferredRecoveryReplayAdapter {
    fn replay_record(
        &mut self,
        ctx: &mut ReplayContext,
        record: &WalRecord,
    ) -> AndromedaResult<()> {
        let manifest_before = (record.header.kind == WalRecordKind::ManifestSwitch)
            .then_some(ctx.active_manifest)
            .flatten();
        let trace_count_before = ctx.manifest_switch_traces.len();

        replay_wal_record(ctx, record)?;

        if record.header.kind == WalRecordKind::ManifestSwitch {
            validate_manifest_switch_replay_result(ctx, manifest_before, trace_count_before)?;
        }

        Ok(())
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

fn validate_manifest_switch_replay_result(
    ctx: &ReplayContext,
    previous_manifest: Option<DatabaseManifest>,
    trace_count_before: usize,
) -> AndromedaResult<()> {
    let Some(trace) = ctx.manifest_switch_traces.get(trace_count_before) else {
        return Err(recovery_error(
            "ManifestSwitch replay produced no recovery trace; recovery cannot open without observable switch evidence",
        ));
    };

    match *trace {
        ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
            lsn,
            manifest_version,
            reason,
        } => Err(recovery_error(format!(
            "ManifestSwitch at LSN {} for manifest version {} failed validation: {}",
            lsn.get(),
            manifest_version,
            reason
        ))),
        ManifestSwitchRecoveryTrace::ManifestSwitchApplied {
            lsn,
            manifest_version,
            base_checkpoint_lsn,
            required_wal_start_lsn,
            ..
        } => {
            if let Some(previous) = previous_manifest {
                if manifest_version <= previous.manifest_version {
                    return Err(recovery_error(format!(
                        "ManifestSwitch at LSN {} regresses manifest version: previous={}, next={}",
                        lsn.get(),
                        previous.manifest_version,
                        manifest_version
                    )));
                }
                if base_checkpoint_lsn <= previous.base_checkpoint_lsn {
                    return Err(recovery_error(format!(
                        "ManifestSwitch at LSN {} must strictly advance base checkpoint LSN: previous={}, next={}",
                        lsn.get(),
                        previous.base_checkpoint_lsn.get(),
                        base_checkpoint_lsn.get()
                    )));
                }
                if required_wal_start_lsn < previous.required_wal_start_lsn {
                    return Err(recovery_error(format!(
                        "ManifestSwitch at LSN {} regresses required WAL start LSN: previous={}, next={}",
                        lsn.get(),
                        previous.required_wal_start_lsn.get(),
                        required_wal_start_lsn.get()
                    )));
                }
            }
            Ok(())
        },
    }
}

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

fn validate_bootstrap_redo_boundary(
    manifest: &DatabaseManifest,
    durable_records: &[WalRecord],
) -> AndromedaResult<()> {
    if manifest.required_wal_start_lsn.is_zero() && !manifest.base_checkpoint_lsn.is_zero() {
        return Err(recovery_error(
            "bootstrap ZERO redo boundary requires a bootstrap snapshot",
        ));
    }

    if manifest.required_wal_start_lsn.is_zero()
        && durable_records
            .first()
            .is_some_and(|record| record.header.lsn != Lsn::new(1))
    {
        return Err(recovery_error(
            "bootstrap ZERO redo boundary WAL prefix must start at LSN 1",
        ));
    }

    Ok(())
}

fn validate_conflicting_terminal_records(durable_records: &[WalRecord]) -> AndromedaResult<()> {
    let mut terminal_by_tx = std::collections::BTreeMap::new();
    for record in durable_records {
        let Some(transaction_id) = record.header.transaction_id else {
            continue;
        };
        if !matches!(
            record.header.kind,
            WalRecordKind::TxCommit | WalRecordKind::TxRollback
        ) {
            continue;
        }

        if let Some(previous_kind) = terminal_by_tx.insert(transaction_id, record.header.kind) {
            return Err(recovery_error(format!(
                "duplicate or conflicting terminal records for transaction {}: {:?} then {:?}",
                transaction_id.get(),
                previous_kind,
                record.header.kind
            )));
        }
    }

    Ok(())
}
