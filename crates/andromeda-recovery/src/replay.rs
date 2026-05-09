use std::collections::HashMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_wal::{DurableTransactionState, Lsn, WalRecord, WalRecordKind};

use crate::{ConceptualRedoPlan, RedoRecordDecision, RedoRecordPlan, StartupMode};

/// Handler result for a single WAL record replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// Record was successfully applied to the database state.
    Applied,
    /// Record was skipped in this recovery context.
    Skipped,
    /// Record was validated, but durable inline index redo is not promoted.
    /// Recovery must rebuild the affected index before normal writes resume.
    IndexRebuildRequired,
    /// Record handler is not promoted; recovery must fail-stop if records of
    /// this type require redo.
    NotYetImplemented,
    /// Record is deprecated and should not appear in new WAL files.
    Deprecated,
}

impl ReplayOutcome {
    pub const fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::NotYetImplemented | Self::Deprecated)
    }
}

/// Result of replaying a single WAL record with metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub outcome: ReplayOutcome,
    pub error: Option<String>,
}

impl ReplayResult {
    pub fn applied(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Applied,
            error: None,
        }
    }

    pub fn skipped(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Skipped,
            error: None,
        }
    }

    pub fn index_rebuild_required(
        lsn: Lsn,
        kind: WalRecordKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::IndexRebuildRequired,
            error: Some(message.into()),
        }
    }

    pub fn deferred(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::NotYetImplemented,
            error: Some(format!(
                "{:?} recovery handler is not promoted; redo payload decoding and idempotent apply semantics must be implemented before records of this type can replay.",
                kind
            )),
        }
    }

    pub fn deprecated(lsn: Lsn, kind: WalRecordKind) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::Deprecated,
            error: Some(format!(
                "{:?} is deprecated and should not appear in new WAL files.",
                kind
            )),
        }
    }

    pub fn error(lsn: Lsn, kind: WalRecordKind, error_msg: impl Into<String>) -> Self {
        Self {
            lsn,
            kind,
            outcome: ReplayOutcome::NotYetImplemented,
            error: Some(error_msg.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRebuildRequiredEvidence {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<TransactionId>,
    pub index_id: u64,
    pub key_format_major: u32,
    pub key_format_minor: u32,
    pub codec_version: u8,
    pub max_key_size: u16,
    pub payload_len: usize,
    pub payload_checksum: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSwitchRecoveryTrace {
    ManifestSwitchApplied {
        lsn: Lsn,
        manifest_version: u64,
        snapshot_id: u64,
        base_checkpoint_lsn: Lsn,
        required_wal_start_lsn: Lsn,
    },
    ManifestSwitchValidationFailed {
        lsn: Lsn,
        manifest_version: u64,
        reason: &'static str,
    },
}

/// Summary report produced at the end of a single WAL replay session.
///
/// All counters are derived exclusively from the durable WAL prefix and the
/// conceptual redo plan. RAM-only state must not be reflected here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalReplayReport {
    /// Recovery startup mode that governed this session.
    pub startup_mode: StartupMode,
    /// LSN from which redo started.
    pub replay_start_lsn: Lsn,
    /// Highest LSN where a handler returned `Applied`.
    pub replay_end_lsn: Option<Lsn>,
    /// Total plan records evaluated.
    pub total_records: usize,
    /// Records whose handler returned `Applied`.
    pub applied_count: usize,
    /// Records whose handler returned `Skipped`.
    pub handler_skipped_count: usize,
    /// Records whose redo decision was anything other than `Replay`.
    pub plan_skipped_count: usize,
    /// Records whose handler returned `NotYetImplemented`.
    pub not_yet_implemented_count: usize,
    /// Index/B-Tree records requiring a rebuild before access paths are trusted.
    pub index_rebuild_required_count: usize,
    /// Structured access-path rebuild evidence emitted by this replay session.
    pub index_rebuild_required: Vec<IndexRebuildRequiredEvidence>,
    /// Incomplete transactions identified and discarded by the plan.
    pub incomplete_transaction_count: usize,
    /// Transactions whose `TxCommit` record was found in durable WAL.
    pub committed_transaction_count: usize,
    /// `true` if any handler returned `NotYetImplemented` or `Deprecated`.
    pub has_replay_errors: bool,
}

impl WalReplayReport {
    /// Returns `true` when recovery can open without discarded transactions,
    /// replay errors, or pending access-path rebuilds.
    pub fn is_clean_recovery(&self) -> bool {
        self.incomplete_transaction_count == 0
            && !self.has_replay_errors
            && !self.requires_access_path_rebuild()
    }

    /// Returns `true` when incomplete transactions were discarded.
    pub fn has_discarded_transactions(&self) -> bool {
        self.incomplete_transaction_count > 0
    }

    /// Returns `true` when one or more access paths must be rebuilt.
    pub const fn requires_access_path_rebuild(&self) -> bool {
        self.index_rebuild_required_count > 0
    }

    /// Returns structured access-path rebuild evidence emitted by this replay.
    pub fn access_path_rebuild_evidence(&self) -> &[IndexRebuildRequiredEvidence] {
        &self.index_rebuild_required
    }

    /// Returns `true` when any redo records are queued for replay by the plan.
    pub fn has_replay_work(&self) -> bool {
        self.applied_count > 0
            || self.not_yet_implemented_count > 0
            || self.index_rebuild_required_count > 0
    }
}

/// Minimal replay target state required by the generic redo driver.
///
/// Concrete storage/page/heap/index/catalog apply state stays in the owner
/// crate. The driver only observes counters and durable boundary evidence.
pub trait RecoveryReplayTarget {
    fn applied_count(&self) -> usize;
    fn skipped_count(&self) -> usize;
    fn error_records(&self) -> &[ReplayResult];
    fn index_rebuild_required(&self) -> &[IndexRebuildRequiredEvidence];
    fn observe_checkpoint_end(&mut self, lsn: Lsn);
    fn require_manifest_switch_checkpoint_evidence(&mut self);
}

/// Concrete replay adapter for owner-specific WAL handlers.
pub trait RecoveryWalReplayAdapter<Target: RecoveryReplayTarget> {
    fn replay_record(&mut self, target: &mut Target, record: &WalRecord) -> AndromedaResult<()>;

    fn is_explicit_deferred_replay_error(&self, _target: &Target, _record: &WalRecord) -> bool {
        false
    }
}

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

fn build_replay_index<'a, Target: RecoveryReplayTarget>(
    durable_records: &'a [WalRecord],
    target: &mut Target,
) -> AndromedaResult<HashMap<Lsn, &'a WalRecord>> {
    let mut by_lsn: HashMap<Lsn, &WalRecord> = HashMap::with_capacity(durable_records.len());
    for record in durable_records {
        record.validate()?;
        if record.header.kind == WalRecordKind::CheckpointEnd {
            target.observe_checkpoint_end(record.header.lsn);
        }
        if by_lsn.insert(record.header.lsn, record).is_some() {
            return Err(recovery_error(format!(
                "durable WAL replay input contains duplicate LSN {}",
                record.header.lsn.get()
            )));
        }
    }

    Ok(by_lsn)
}

fn validate_planned_replay_record(
    redo_rec: &RedoRecordPlan,
    record: &WalRecord,
) -> AndromedaResult<()> {
    if record.header.kind != redo_rec.kind {
        return Err(recovery_error(format!(
            "redo plan LSN {} expects {:?} but durable WAL contains {:?}",
            redo_rec.lsn.get(),
            redo_rec.kind,
            record.header.kind
        )));
    }
    if record.header.transaction_id != redo_rec.transaction_id {
        return Err(recovery_error(format!(
            "redo plan LSN {} transaction evidence does not match durable WAL",
            redo_rec.lsn.get()
        )));
    }
    match (record.header.transaction_id, redo_rec.transaction_state) {
        (Some(_), Some(DurableTransactionState::Committed)) => Ok(()),
        (Some(_), Some(state)) => Err(recovery_error(format!(
            "redo plan attempted to replay non-committed transaction record at LSN {}: state={state:?}",
            redo_rec.lsn.get()
        ))),
        (Some(_), None) => Err(recovery_error(format!(
            "redo plan attempted to replay transaction record at LSN {} without commit evidence",
            redo_rec.lsn.get()
        ))),
        (None, _) => Ok(()),
    }
}

fn recovery_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::TransactionId;

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
