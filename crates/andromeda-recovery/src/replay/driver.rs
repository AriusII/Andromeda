use std::collections::HashMap;

use andromeda_error::AndromedaResult;
use andromeda_wal::{Lsn, WalRecord};

use crate::{ConceptualRedoPlan, RedoRecordDecision, RedoRecordPlan};

use super::validation::{build_replay_index, validate_planned_replay_record};
use super::{
    IndexRebuildRequiredEvidence, RecoveryReplayTarget, RecoveryWalReplayAdapter, WalReplayReport,
    recovery_error,
};

/// Execute a conceptual redo plan using an owner-provided replay adapter.
///
/// This owns the generic replay-loop invariants: plan ordering, by-LSN lookup,
/// plan/input consistency, transaction-state checks, checkpoint observation,
/// and counter/report assembly. The adapter owns concrete record application.
pub fn execute_redo_plan_with_adapter<Target, Adapter>(
    plan: &ConceptualRedoPlan,
    durable_records: &[WalRecord],
    target: &mut Target,
    adapter: &mut Adapter,
) -> AndromedaResult<WalReplayReport>
where
    Target: RecoveryReplayTarget,
    Adapter: RecoveryWalReplayAdapter<Target>,
{
    let baseline = ReplaySessionBaseline::capture(target);
    let mut driver = RedoPlanReplayDriver::from_durable_records(durable_records, target)?;
    target.require_manifest_switch_checkpoint_evidence();
    driver.execute(plan, target, adapter)?;

    let committed_transaction_count = plan
        .transaction_evidence
        .iter()
        .filter(|t| t.commit_lsn.is_some())
        .count();

    let new_index_rebuild_required = baseline.new_index_rebuild_required(target);
    let index_rebuild_required_count = new_index_rebuild_required.len();

    Ok(WalReplayReport {
        startup_mode: plan.startup_mode,
        replay_start_lsn: plan.redo_from_lsn,
        replay_end_lsn: driver.replay_end_lsn(),
        total_records: plan.records.len(),
        applied_count: baseline.applied_delta(target),
        handler_skipped_count: baseline.skipped_delta(target),
        plan_skipped_count: driver.plan_skipped_count(),
        not_yet_implemented_count: baseline.error_delta(target),
        index_rebuild_required_count,
        index_rebuild_required: new_index_rebuild_required,
        incomplete_transaction_count: plan.incomplete_transactions.len(),
        committed_transaction_count,
        has_replay_errors: baseline.has_new_errors(target),
    })
}

struct RedoPlanReplayDriver<'a> {
    by_lsn: HashMap<Lsn, &'a WalRecord>,
    plan_skipped_count: usize,
    replay_end_lsn: Option<Lsn>,
    previous_plan_lsn: Option<Lsn>,
}

impl<'a> RedoPlanReplayDriver<'a> {
    fn from_durable_records<Target: RecoveryReplayTarget>(
        durable_records: &'a [WalRecord],
        target: &mut Target,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            by_lsn: build_replay_index(durable_records, target)?,
            plan_skipped_count: 0,
            replay_end_lsn: None,
            previous_plan_lsn: None,
        })
    }

    fn execute<Target, Adapter>(
        &mut self,
        plan: &ConceptualRedoPlan,
        target: &mut Target,
        adapter: &mut Adapter,
    ) -> AndromedaResult<()>
    where
        Target: RecoveryReplayTarget,
        Adapter: RecoveryWalReplayAdapter<Target>,
    {
        for redo_rec in plan.records.iter() {
            self.validate_next_plan_lsn(redo_rec.lsn)?;

            match redo_rec.decision {
                RedoRecordDecision::Replay => {
                    self.replay_planned_record(target, adapter, redo_rec)?
                },
                _ => self.plan_skipped_count += 1,
            }
        }

        Ok(())
    }

    const fn replay_end_lsn(&self) -> Option<Lsn> {
        self.replay_end_lsn
    }

    const fn plan_skipped_count(&self) -> usize {
        self.plan_skipped_count
    }

    fn validate_next_plan_lsn(&mut self, current_lsn: Lsn) -> AndromedaResult<()> {
        match self.previous_plan_lsn {
            Some(previous) if current_lsn <= previous => {
                return Err(recovery_error(format!(
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

    fn replay_planned_record<Target, Adapter>(
        &mut self,
        target: &mut Target,
        adapter: &mut Adapter,
        redo_rec: &RedoRecordPlan,
    ) -> AndromedaResult<()>
    where
        Target: RecoveryReplayTarget,
        Adapter: RecoveryWalReplayAdapter<Target>,
    {
        let Some(&record) = self.by_lsn.get(&redo_rec.lsn) else {
            return Err(recovery_error(format!(
                "redo plan references LSN {} but the durable WAL input does not contain it",
                redo_rec.lsn.get()
            )));
        };
        validate_planned_replay_record(redo_rec, record)?;

        let applied_before = target.applied_count();
        match adapter.replay_record(target, record) {
            Ok(()) => {},
            Err(_err) if adapter.is_explicit_deferred_replay_error(target, record) => {},
            Err(err) => return Err(err),
        }

        if target.applied_count() > applied_before {
            self.replay_end_lsn = Some(record.header.lsn);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct ReplaySessionBaseline {
    applied_count: usize,
    skipped_count: usize,
    error_count: usize,
    index_rebuild_required_count: usize,
}

impl ReplaySessionBaseline {
    fn capture(target: &impl RecoveryReplayTarget) -> Self {
        Self {
            applied_count: target.applied_count(),
            skipped_count: target.skipped_count(),
            error_count: target.error_records().len(),
            index_rebuild_required_count: target.index_rebuild_required().len(),
        }
    }

    fn applied_delta(self, target: &impl RecoveryReplayTarget) -> usize {
        target.applied_count() - self.applied_count
    }

    fn skipped_delta(self, target: &impl RecoveryReplayTarget) -> usize {
        target.skipped_count() - self.skipped_count
    }

    fn error_delta(self, target: &impl RecoveryReplayTarget) -> usize {
        target.error_records().len() - self.error_count
    }

    fn has_new_errors(self, target: &impl RecoveryReplayTarget) -> bool {
        target.error_records().len() > self.error_count
    }

    fn new_index_rebuild_required(
        self,
        target: &impl RecoveryReplayTarget,
    ) -> Vec<IndexRebuildRequiredEvidence> {
        target.index_rebuild_required()[self.index_rebuild_required_count..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use andromeda_types::TransactionId;
    use andromeda_wal::{WalRecord, WalRecordKind};

    use super::*;
    use crate::{ReplayResult, StartupMode};

    #[derive(Default)]
    struct DummyTarget {
        applied_count: usize,
        skipped_count: usize,
        error_records: Vec<ReplayResult>,
        index_rebuild_required: Vec<IndexRebuildRequiredEvidence>,
        checkpoint_end: Option<Lsn>,
        requires_checkpoint: bool,
    }

    impl RecoveryReplayTarget for DummyTarget {
        fn applied_count(&self) -> usize {
            self.applied_count
        }

        fn skipped_count(&self) -> usize {
            self.skipped_count
        }

        fn error_records(&self) -> &[ReplayResult] {
            &self.error_records
        }

        fn index_rebuild_required(&self) -> &[IndexRebuildRequiredEvidence] {
            &self.index_rebuild_required
        }

        fn observe_checkpoint_end(&mut self, lsn: Lsn) {
            self.checkpoint_end = Some(lsn);
        }

        fn require_manifest_switch_checkpoint_evidence(&mut self) {
            self.requires_checkpoint = true;
        }
    }

    struct DummyAdapter;

    impl RecoveryWalReplayAdapter<DummyTarget> for DummyAdapter {
        fn replay_record(
            &mut self,
            target: &mut DummyTarget,
            record: &WalRecord,
        ) -> AndromedaResult<()> {
            match record.header.kind {
                WalRecordKind::RowInsert => target.applied_count += 1,
                _ => target.skipped_count += 1,
            }
            Ok(())
        }
    }

    fn record(kind: WalRecordKind, lsn: u64, tx: Option<u64>) -> WalRecord {
        WalRecord::from_parts(
            kind,
            Lsn::new(lsn),
            (lsn > 1).then(|| Lsn::new(lsn - 1)),
            tx.map(TransactionId::new),
            Vec::new(),
        )
        .unwrap()
    }

    #[test]
    fn generic_replay_driver_counts_plan_and_handler_results() {
        let records = vec![
            record(WalRecordKind::TxBegin, 1, Some(7)),
            record(WalRecordKind::RowInsert, 2, Some(7)),
            record(WalRecordKind::TxCommit, 3, Some(7)),
        ];
        let plan = crate::RecoveryPlan {
            startup_mode: StartupMode::SafeStart,
            mounted_snapshot_id: 10,
            redo_from_lsn: Lsn::new(1),
            discard_incomplete_transactions: true,
        }
        .build_redo_plan(&records)
        .unwrap();
        let mut target = DummyTarget::default();
        let mut adapter = DummyAdapter;

        let report =
            execute_redo_plan_with_adapter(&plan, &records, &mut target, &mut adapter).unwrap();

        assert_eq!(report.total_records, 3);
        assert_eq!(report.applied_count, 1);
        assert_eq!(report.plan_skipped_count, 2);
        assert_eq!(report.committed_transaction_count, 1);
        assert!(target.requires_checkpoint);
    }
}
