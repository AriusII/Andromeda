use std::collections::HashMap;

use andromeda_core::AndromedaResult;

use crate::{DatabaseManifest, DurableTransactionState, Lsn, WalRecord, WalRecordKind};

use super::super::planning::{ConceptualRedoPlan, RedoRecordDecision, RedoRecordPlan};
use super::super::replay::{
    IndexRebuildRequiredEvidence, ManifestSwitchRecoveryTrace, ReplayContext, ReplayOutcome,
    ReplayResult, replay_wal_record_result,
};
use super::super::storage_error;

pub(super) struct RedoPlanReplayDriver<'a> {
    by_lsn: HashMap<Lsn, &'a WalRecord>,
    plan_skipped_count: usize,
    replay_end_lsn: Option<Lsn>,
    previous_plan_lsn: Option<Lsn>,
}

impl<'a> RedoPlanReplayDriver<'a> {
    pub(super) fn from_durable_records(
        durable_records: &'a [WalRecord],
        ctx: &mut ReplayContext,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            by_lsn: build_replay_index(durable_records, ctx)?,
            plan_skipped_count: 0,
            replay_end_lsn: None,
            previous_plan_lsn: None,
        })
    }

    pub(super) fn execute(
        &mut self,
        plan: &ConceptualRedoPlan,
        ctx: &mut ReplayContext,
    ) -> AndromedaResult<()> {
        for redo_rec in plan.records.iter() {
            self.validate_next_plan_lsn(redo_rec.lsn)?;

            match redo_rec.decision {
                RedoRecordDecision::Replay => self.replay_planned_record(ctx, redo_rec)?,
                _ => self.plan_skipped_count += 1,
            }
        }

        Ok(())
    }

    pub(super) const fn replay_end_lsn(&self) -> Option<Lsn> {
        self.replay_end_lsn
    }

    pub(super) const fn plan_skipped_count(&self) -> usize {
        self.plan_skipped_count
    }

    fn validate_next_plan_lsn(&mut self, current_lsn: Lsn) -> AndromedaResult<()> {
        match self.previous_plan_lsn {
            Some(previous) if current_lsn <= previous => {
                return Err(storage_error(format!(
                    "redo plan records must be strictly ordered by LSN: previous={}, current={}",
                    previous.get(),
                    current_lsn.get()
                )));
            },
            _ => {},
        }
        self.previous_plan_lsn = Some(current_lsn);
        Ok(())
    }

    fn replay_planned_record(
        &mut self,
        ctx: &mut ReplayContext,
        redo_rec: &RedoRecordPlan,
    ) -> AndromedaResult<()> {
        let Some(&record) = self.by_lsn.get(&redo_rec.lsn) else {
            return Err(storage_error(format!(
                "redo plan references LSN {} but the durable WAL input does not contain it",
                redo_rec.lsn.get()
            )));
        };
        validate_planned_replay_record(redo_rec, record)?;

        let applied_before = ctx.applied_count;
        let manifest_before = (record.header.kind == WalRecordKind::ManifestSwitch)
            .then_some(ctx.active_manifest)
            .flatten();
        let manifest_trace_count_before = ctx.manifest_switch_traces.len();
        match replay_wal_record_result(ctx, record) {
            Ok(_) => {},
            Err(_err) if is_explicit_deferred_replay_error(ctx, record) => {},
            Err(err) => return Err(err),
        }
        if record.header.kind == WalRecordKind::ManifestSwitch {
            validate_manifest_switch_replay_result(
                ctx,
                manifest_before,
                manifest_trace_count_before,
            )?;
        }

        if ctx.applied_count > applied_before {
            self.replay_end_lsn = Some(record.header.lsn);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ReplaySessionBaseline {
    applied_count: usize,
    skipped_count: usize,
    error_count: usize,
    index_rebuild_required_count: usize,
}

impl ReplaySessionBaseline {
    pub(super) fn capture(ctx: &ReplayContext) -> Self {
        Self {
            applied_count: ctx.applied_count,
            skipped_count: ctx.skipped_count,
            error_count: ctx.error_records.len(),
            index_rebuild_required_count: ctx.index_rebuild_required.len(),
        }
    }

    pub(super) fn applied_delta(self, ctx: &ReplayContext) -> usize {
        ctx.applied_count - self.applied_count
    }

    pub(super) fn skipped_delta(self, ctx: &ReplayContext) -> usize {
        ctx.skipped_count - self.skipped_count
    }

    pub(super) fn error_delta(self, ctx: &ReplayContext) -> usize {
        ctx.error_records.len() - self.error_count
    }

    pub(super) fn has_new_errors(self, ctx: &ReplayContext) -> bool {
        ctx.error_records.len() > self.error_count
    }

    pub(super) fn new_index_rebuild_required(
        self,
        ctx: &ReplayContext,
    ) -> Vec<IndexRebuildRequiredEvidence> {
        ctx.index_rebuild_required[self.index_rebuild_required_count..].to_vec()
    }
}

fn build_replay_index<'a>(
    durable_records: &'a [WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<HashMap<Lsn, &'a WalRecord>> {
    let mut by_lsn: HashMap<Lsn, &WalRecord> = HashMap::with_capacity(durable_records.len());
    for record in durable_records {
        record.validate()?;
        if record.header.kind == WalRecordKind::CheckpointEnd {
            ctx.observe_checkpoint_end(record.header.lsn);
        }
        if by_lsn.insert(record.header.lsn, record).is_some() {
            return Err(storage_error(format!(
                "durable WAL replay input contains duplicate LSN {}",
                record.header.lsn.get()
            )));
        }
    }

    Ok(by_lsn)
}

fn validate_manifest_switch_replay_result(
    ctx: &ReplayContext,
    previous_manifest: Option<DatabaseManifest>,
    trace_count_before: usize,
) -> AndromedaResult<()> {
    let Some(trace) = ctx.manifest_switch_traces.get(trace_count_before) else {
        return Err(storage_error(
            "ManifestSwitch replay produced no recovery trace; recovery cannot open without observable switch evidence",
        ));
    };

    match *trace {
        ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
            lsn,
            manifest_version,
            reason,
        } => Err(storage_error(format!(
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
                    return Err(storage_error(format!(
                        "ManifestSwitch at LSN {} regresses manifest version: previous={}, next={}",
                        lsn.get(),
                        previous.manifest_version,
                        manifest_version
                    )));
                }
                if base_checkpoint_lsn <= previous.base_checkpoint_lsn {
                    return Err(storage_error(format!(
                        "ManifestSwitch at LSN {} must strictly advance base checkpoint LSN: previous={}, next={}",
                        lsn.get(),
                        previous.base_checkpoint_lsn.get(),
                        base_checkpoint_lsn.get()
                    )));
                }
                if required_wal_start_lsn < previous.required_wal_start_lsn {
                    return Err(storage_error(format!(
                        "ManifestSwitch at LSN {} regresses required WAL start LSN: previous={}, next={}",
                        lsn.get(),
                        previous.required_wal_start_lsn.get(),
                        required_wal_start_lsn.get()
                    )));
                }
                if lsn <= previous.base_checkpoint_lsn {
                    return Err(storage_error(format!(
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

fn validate_planned_replay_record(
    redo_rec: &RedoRecordPlan,
    record: &WalRecord,
) -> AndromedaResult<()> {
    if record.header.kind != redo_rec.kind {
        return Err(storage_error(format!(
            "redo plan LSN {} expects {:?} but durable WAL contains {:?}",
            redo_rec.lsn.get(),
            redo_rec.kind,
            record.header.kind
        )));
    }
    if record.header.transaction_id != redo_rec.transaction_id {
        return Err(storage_error(format!(
            "redo plan LSN {} transaction evidence does not match durable WAL",
            redo_rec.lsn.get()
        )));
    }
    match (record.header.transaction_id, redo_rec.transaction_state) {
        (Some(_), Some(DurableTransactionState::Committed)) => Ok(()),
        (Some(_), Some(state)) => Err(storage_error(format!(
            "redo plan attempted to replay non-committed transaction record at LSN {}: state={state:?}",
            redo_rec.lsn.get()
        ))),
        (Some(_), None) => Err(storage_error(format!(
            "redo plan attempted to replay transaction record at LSN {} without commit evidence",
            redo_rec.lsn.get()
        ))),
        (None, _) => Ok(()),
    }
}

fn is_explicit_deferred_replay_error(ctx: &ReplayContext, record: &WalRecord) -> bool {
    let Some(result) = ctx.error_records.last() else {
        return false;
    };
    result.lsn == record.header.lsn
        && result.kind == record.header.kind
        && result.outcome == ReplayOutcome::NotYetImplemented
        && is_explicit_deferred_kind(result)
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
