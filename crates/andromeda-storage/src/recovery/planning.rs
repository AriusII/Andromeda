use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

use crate::{
    summarize_transactions_from_records, DatabaseManifest, DurableTransactionResume,
    DurableTransactionState, IncompleteDurableTransaction, Lsn, WalRecord, WalRecordKind,
    WalScanResult, WalScanStop, WalScanStopReason,
};

use super::{
    coverage::{validate_wal_coverage, WalCoverageEvidence},
    storage_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMode {
    FastStart,
    SafeStart,
    ForensicStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryPlan {
    pub startup_mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub redo_from_lsn: Lsn,
    pub discard_incomplete_transactions: bool,
}

impl RecoveryPlan {
    pub fn from_manifest(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
    ) -> AndromedaResult<Self> {
        manifest.validate()?;

        Ok(Self {
            startup_mode,
            mounted_snapshot_id: manifest.snapshot_id,
            redo_from_lsn: manifest.required_wal_start_lsn,
            discard_incomplete_transactions: true,
        })
    }

    pub fn from_manifest_and_wal(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        Self::from_manifest(manifest, startup_mode)?.build_redo_plan(durable_records)
    }

    pub fn from_manifest_and_wal_scan(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
        scan: &WalScanResult,
    ) -> AndromedaResult<ConceptualRedoPlan> {
        if matches!(
            scan.stopped.map(|stop| stop.reason),
            Some(
                WalScanStopReason::LsnGap
                    | WalScanStopReason::DuplicateOrReorderedLsn
                    | WalScanStopReason::PreviousLsnMismatch
            )
        ) {
            return Err(storage_error(
                "recovery WAL scan stopped at a non-recoverable LSN chain boundary",
            ));
        }

        let mut plan =
            Self::from_manifest(manifest, startup_mode)?.build_redo_plan(&scan.records)?;
        plan.wal_scan_stop = scan.stopped;
        Ok(plan)
    }

    pub fn build_redo_plan(
        self,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        for record in durable_records {
            record.validate()?;
        }
        let coverage = validate_wal_coverage(self.redo_from_lsn, durable_records)?;

        let durable_lsn = durable_records
            .iter()
            .map(|record| record.header.lsn)
            .max()
            .unwrap_or_default();
        let transaction_evidence = summarize_transactions_from_records(durable_records);
        let incomplete_transactions = transaction_evidence
            .iter()
            .filter(|summary| summary.is_incomplete())
            .map(|summary| IncompleteDurableTransaction {
                transaction_id: summary.transaction_id,
                first_lsn: summary.first_lsn,
                last_lsn: summary.last_lsn,
                record_count: summary.record_count,
            })
            .collect();

        let records = durable_records
            .iter()
            .map(|record| RedoRecordPlan {
                lsn: record.header.lsn,
                kind: record.header.kind,
                transaction_id: record.header.transaction_id,
                transaction_state: record.header.transaction_id.and_then(|transaction_id| {
                    transaction_evidence
                        .iter()
                        .find(|summary| summary.transaction_id == transaction_id)
                        .map(|summary| summary.state)
                }),
                decision: redo_decision_for_record(self, record, &transaction_evidence),
            })
            .collect();

        Ok(ConceptualRedoPlan {
            startup_mode: self.startup_mode,
            mounted_snapshot_id: self.mounted_snapshot_id,
            redo_from_lsn: self.redo_from_lsn,
            durable_lsn,
            discard_incomplete_transactions: self.discard_incomplete_transactions,
            coverage,
            transaction_evidence,
            incomplete_transactions,
            records,
            wal_scan_stop: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedoRecordDecision {
    Replay,
    SkipBeforeRedoStart,
    SkipIncompleteTransaction,
    SkipRolledBackTransaction,
    SkipMissingCommitEvidence,
    SkipNonRedoRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedoRecordPlan {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<andromeda_core::TransactionId>,
    pub transaction_state: Option<DurableTransactionState>,
    pub decision: RedoRecordDecision,
}

impl RedoRecordPlan {
    pub const fn should_replay(self) -> bool {
        matches!(self.decision, RedoRecordDecision::Replay)
    }
}

/// A conceptual redo plan derived from a manifest plus the durable WAL prefix.
///
/// The plan exposes three explicit boundary slots that the recovery executor
/// must keep distinct:
///
/// 1. **Mounted cold snapshot** — `mounted_snapshot_id` identifies the
///    durable, on-disk snapshot that anchors recovery. This is the only
///    source of pre-WAL truth; RAM/hot state never participates here.
/// 2. **WAL replay range** — `redo_from_lsn` (inclusive) up to `durable_lsn`
///    (inclusive) describes which durable WAL records are eligible for
///    consideration. `coverage` proves the LSN chain is contiguous and
///    anchored to the manifest start.
/// 3. **Unavailable / corrupt WAL segment boundary** — `wal_scan_stop` is
///    `Some` when the durable WAL had a recoverable tail boundary
///    (truncated/corrupt suffix) at exactly `durable_lsn`. Forensic chain
///    breaks (gap / duplicate / previous-LSN mismatch) are rejected before
///    a plan is constructed and never surface here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptualRedoPlan {
    pub startup_mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub redo_from_lsn: Lsn,
    pub durable_lsn: Lsn,
    pub discard_incomplete_transactions: bool,
    pub coverage: WalCoverageEvidence,
    pub transaction_evidence: Vec<DurableTransactionResume>,
    pub incomplete_transactions: Vec<IncompleteDurableTransaction>,
    pub records: Vec<RedoRecordPlan>,
    pub wal_scan_stop: Option<WalScanStop>,
}

impl ConceptualRedoPlan {
    pub fn replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.records
            .iter()
            .filter(|record| record.should_replay())
            .map(|record| record.lsn)
    }

    pub fn committed_redo_records(&self) -> impl Iterator<Item = &RedoRecordPlan> + '_ {
        self.records.iter().filter(|record| {
            record.should_replay()
                && match record.transaction_state {
                    Some(state) => state == DurableTransactionState::Committed,
                    None => true,
                }
        })
    }

    pub fn has_incomplete_transactions(&self) -> bool {
        !self.incomplete_transactions.is_empty()
    }

    pub fn committed_replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.committed_redo_records().map(|record| record.lsn)
    }

    /// Highest transaction id observed in durable WAL evidence.
    ///
    /// Recovery drivers should seed the transaction-id allocator at this floor
    /// (for example via `andromeda_tx::TransactionManager::with_recovered_floor`)
    /// before serving post-restart traffic. This is not RAM truth: it is a
    /// projection of transaction ids present in the durable WAL prefix.
    pub fn recovered_transaction_id_floor(&self) -> u64 {
        self.transaction_evidence
            .iter()
            .map(|summary| summary.transaction_id.get())
            .max()
            .unwrap_or(0)
    }

    pub const fn wal_scan_stop(&self) -> Option<WalScanStop> {
        self.wal_scan_stop
    }

    /// Project the plan's durable boundary into an [`andromeda_observe::RecoveryTrace`]
    /// for emission as a `RecoveryStartup` critical decision.
    ///
    /// The trace carries:
    /// * `last_durable_lsn` — the highest LSN that survived the WAL scan and
    ///   is therefore eligible for replay (cold snapshot + durable WAL truth).
    /// * `corruption_boundary_lsn` — `Some(last_durable_lsn)` iff the WAL
    ///   scan stopped at a recoverable tail boundary (truncated / corrupt
    ///   suffix). `None` indicates a clean scan with no observed boundary.
    pub fn observe_recovery_trace(&self, trace_id: TraceId) -> andromeda_observe::RecoveryTrace {
        andromeda_observe::RecoveryTrace {
            trace_id,
            last_durable_lsn: self.durable_lsn.get(),
            corruption_boundary_lsn: self.wal_scan_stop.map(|_| self.durable_lsn.get()),
        }
    }
}

fn redo_decision_for_record(
    plan: RecoveryPlan,
    record: &WalRecord,
    transaction_evidence: &[DurableTransactionResume],
) -> RedoRecordDecision {
    if record.header.lsn < plan.redo_from_lsn {
        return RedoRecordDecision::SkipBeforeRedoStart;
    }

    if !record.header.kind.is_redo_relevant() {
        return RedoRecordDecision::SkipNonRedoRecord;
    }

    if let Some(transaction_id) = record.header.transaction_id {
        let Some(summary) = transaction_evidence
            .iter()
            .find(|summary| summary.transaction_id == transaction_id)
        else {
            return RedoRecordDecision::SkipMissingCommitEvidence;
        };

        match summary.state {
            DurableTransactionState::Committed => {}
            DurableTransactionState::RolledBack => {
                return RedoRecordDecision::SkipRolledBackTransaction;
            }
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                if plan.discard_incomplete_transactions {
                    return RedoRecordDecision::SkipIncompleteTransaction;
                }
                return RedoRecordDecision::SkipMissingCommitEvidence;
            }
        }
    }

    RedoRecordDecision::Replay
}
