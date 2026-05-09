//! WAL replay from a manifest-required start LSN during crash recovery.

use andromeda_error::AndromedaResult;
use andromeda_manifest::DatabaseManifest;
use andromeda_wal::{WalRecord, WalRecordKind};

use crate::{
    ConceptualRedoPlan, ManifestSwitchRecoveryTrace, RecoveryPlan, RecoveryWalReplayAdapter,
    ReplayContext, ReplayOutcome, ReplayResult, StartupMode, WalReplayReport,
    execute_redo_plan_with_adapter, replay_wal_record_result,
};

mod validation;

use validation::{
    validate_bootstrap_redo_boundary, validate_conflicting_terminal_records,
    validate_durable_replay_chain,
};

pub fn replay_wal_from_lsn(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    durable_records: &[WalRecord],
) -> AndromedaResult<WalReplayReport> {
    let mut ctx = ReplayContext::new();
    replay_wal_from_lsn_into_context(manifest, startup_mode, durable_records, &mut ctx)
}

pub fn replay_wal_from_lsn_into_context(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    validate_durable_replay_chain(durable_records)?;
    validate_bootstrap_redo_boundary(manifest, durable_records)?;
    let plan = RecoveryPlan::from_manifest_and_wal(manifest, startup_mode, durable_records)?;
    if ctx.active_manifest.is_none() {
        ctx.active_manifest = Some(*manifest);
    }
    execute_redo_plan_into_context(&plan, durable_records, ctx)
}

pub fn execute_redo_plan(
    plan: &ConceptualRedoPlan,
    durable_records: &[WalRecord],
) -> AndromedaResult<WalReplayReport> {
    let mut ctx = ReplayContext::new();
    execute_redo_plan_into_context(plan, durable_records, &mut ctx)
}

pub fn execute_redo_plan_into_context(
    plan: &ConceptualRedoPlan,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    validate_conflicting_terminal_records(durable_records)?;
    let mut adapter = RecoveryOwnedWalReplayAdapter;
    execute_redo_plan_with_adapter(plan, durable_records, ctx, &mut adapter)
}

struct RecoveryOwnedWalReplayAdapter;

impl RecoveryWalReplayAdapter<ReplayContext> for RecoveryOwnedWalReplayAdapter {
    fn replay_record(
        &mut self,
        ctx: &mut ReplayContext,
        record: &WalRecord,
    ) -> AndromedaResult<()> {
        let manifest_before = (record.header.kind == WalRecordKind::ManifestSwitch)
            .then_some(ctx.active_manifest)
            .flatten();
        let manifest_trace_count_before = ctx.manifest_switch_traces.len();

        replay_wal_record_result(ctx, record).map(|_| ())?;

        if record.header.kind == WalRecordKind::ManifestSwitch {
            validate_manifest_switch_replay_result(
                ctx,
                manifest_before,
                manifest_trace_count_before,
            )?;
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
            && is_explicit_deferred_kind(result)
    }
}

fn validate_manifest_switch_replay_result(
    ctx: &ReplayContext,
    previous_manifest: Option<DatabaseManifest>,
    trace_count_before: usize,
) -> AndromedaResult<()> {
    let Some(trace) = ctx.manifest_switch_traces.get(trace_count_before) else {
        return Err(crate::recovery_error(
            "ManifestSwitch replay produced no recovery trace; recovery cannot open without observable switch evidence",
        ));
    };

    match *trace {
        ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
            lsn,
            manifest_version,
            reason,
        } => Err(crate::recovery_error(format!(
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
                    return Err(crate::recovery_error(format!(
                        "ManifestSwitch at LSN {} regresses manifest version: previous={}, next={}",
                        lsn.get(),
                        previous.manifest_version,
                        manifest_version
                    )));
                }
                if base_checkpoint_lsn <= previous.base_checkpoint_lsn {
                    return Err(crate::recovery_error(format!(
                        "ManifestSwitch at LSN {} must strictly advance base checkpoint LSN: previous={}, next={}",
                        lsn.get(),
                        previous.base_checkpoint_lsn.get(),
                        base_checkpoint_lsn.get()
                    )));
                }
                if required_wal_start_lsn < previous.required_wal_start_lsn {
                    return Err(crate::recovery_error(format!(
                        "ManifestSwitch at LSN {} regresses required WAL start LSN: previous={}, next={}",
                        lsn.get(),
                        previous.required_wal_start_lsn.get(),
                        required_wal_start_lsn.get()
                    )));
                }
                if lsn <= previous.base_checkpoint_lsn {
                    return Err(crate::recovery_error(format!(
                        "ManifestSwitch record LSN {} must follow the previous base checkpoint LSN {}",
                        lsn.get(),
                        previous.base_checkpoint_lsn.get()
                    )));
                }
            }
            Ok(())
        },
    }
}

fn is_explicit_deferred_kind(result: &ReplayResult) -> bool {
    match result.kind {
        WalRecordKind::PageAllocate
        | WalRecordKind::PageFormat
        | WalRecordKind::IndexInsert
        | WalRecordKind::IndexDelete
        | WalRecordKind::MvccVersionCreate
        | WalRecordKind::MvccVersionClose
        | WalRecordKind::MapDeltaAppend
        | WalRecordKind::CatalogChangeBegin
        | WalRecordKind::CatalogChangeApply
        | WalRecordKind::CatalogChangeCommit
        | WalRecordKind::BTreeInsert
        | WalRecordKind::BTreeDelete
        | WalRecordKind::BTreeSplit
        | WalRecordKind::BTreeMerge => result
            .error
            .as_deref()
            .is_some_and(|message| message.contains("not promoted")),
        _ => false,
    }
}
