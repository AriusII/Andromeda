use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::TraceId;

use crate::{
    summarize_transactions_from_records, DatabaseManifest, DurableTransactionResume,
    DurableTransactionState, IncompleteDurableTransaction, Lsn, WalRecord, WalRecordKind, WalScanResult,
    WalScanStop, WalScanStopReason,
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
pub struct WalCoverageEvidence {
    pub required_wal_start_lsn: Lsn,
    pub first_replay_record_lsn: Option<Lsn>,
    pub last_durable_lsn: Option<Lsn>,
    pub record_count_in_redo_range: usize,
}

fn validate_wal_coverage(
    redo_from_lsn: Lsn,
    durable_records: &[WalRecord],
) -> AndromedaResult<WalCoverageEvidence> {
    if redo_from_lsn.is_zero() {
        return Ok(WalCoverageEvidence {
            required_wal_start_lsn: redo_from_lsn,
            first_replay_record_lsn: durable_records.first().map(|record| record.header.lsn),
            last_durable_lsn: durable_records.last().map(|record| record.header.lsn),
            record_count_in_redo_range: durable_records.len(),
        });
    }

    let mut expected_lsn = None;
    let mut previous_in_redo_range = None;
    let mut saw_redo_start = false;
    let mut first_replay_record_lsn = None;
    let mut last_durable_lsn = None;
    let mut record_count_in_redo_range = 0;

    let mut records_in_redo_range = durable_records
        .iter()
        .filter(|record| record.header.lsn >= redo_from_lsn)
        .peekable();

    while let Some(record) = records_in_redo_range.next() {
        if let Some(expected) = expected_lsn {
            if record.header.lsn < expected {
                return Err(storage_error(
                    "recovery WAL coverage contains duplicate or reordered LSN",
                ));
            }
            if record.header.lsn > expected {
                return Err(storage_error("recovery WAL coverage contains an LSN gap"));
            }
            if record.header.previous_lsn != previous_in_redo_range {
                return Err(storage_error(
                    "recovery WAL coverage previous LSN chain mismatch",
                ));
            }
        } else {
            if record.header.lsn != redo_from_lsn {
                return Err(storage_error(
                    "recovery WAL coverage does not start at required WAL start LSN",
                ));
            }
            if record.header.previous_lsn != expected_previous_lsn_for_recovery_start(redo_from_lsn)
            {
                return Err(storage_error(
                    "recovery WAL coverage start is not anchored to the prior durable LSN",
                ));
            }
            saw_redo_start = true;
            first_replay_record_lsn = Some(record.header.lsn);
        }

        previous_in_redo_range = Some(record.header.lsn);
        last_durable_lsn = Some(record.header.lsn);
        record_count_in_redo_range += 1;
        expected_lsn = if records_in_redo_range.peek().is_some() {
            Some(record.header.lsn.try_next()?)
        } else {
            record.header.lsn.checked_next()
        };
    }

    if !saw_redo_start {
        return Err(storage_error(
            "recovery WAL coverage is missing the required WAL start LSN",
        ));
    }

    Ok(WalCoverageEvidence {
        required_wal_start_lsn: redo_from_lsn,
        first_replay_record_lsn,
        last_durable_lsn,
        record_count_in_redo_range,
    })
}

fn expected_previous_lsn_for_recovery_start(redo_from_lsn: Lsn) -> Option<Lsn> {
    if redo_from_lsn.get() <= 1 {
        None
    } else {
        Some(Lsn::new(redo_from_lsn.get() - 1))
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
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
    pub fn replay_lsns(&self) -> impl Iterator<Item=Lsn> + '_ {
        self.records
            .iter()
            .filter(|record| record.should_replay())
            .map(|record| record.lsn)
    }

    pub fn has_incomplete_transactions(&self) -> bool {
        !self.incomplete_transactions.is_empty()
    }

    pub fn committed_replay_lsns(&self) -> impl Iterator<Item=Lsn> + '_ {
        self.records
            .iter()
            .filter(|record| {
                record.should_replay()
                    && match record.transaction_state {
                    Some(state) => state == DurableTransactionState::Committed,
                    None => true,
                }
            })
            .map(|record| record.lsn)
    }

    pub const fn wal_scan_stop(&self) -> Option<WalScanStop> {
        self.wal_scan_stop
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTrace {
    pub trace_id: TraceId,
    pub startup_mode: StartupMode,
    pub replay_start_lsn: Lsn,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryWal, WalRecordKind};
    use andromeda_core::TransactionId;
    use andromeda_observe::{EventCorrelation, EventEnvelope, EventId, TraceEvent};

    #[test]
    fn manifest_keeps_snapshot_plus_wal_anchor() {
        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(100),
            required_wal_start_lsn: Lsn::new(101),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };

        assert!(manifest.validate().is_ok());
        assert_eq!(
            RecoveryPlan::from_manifest(&manifest, StartupMode::SafeStart)
                .unwrap()
                .redo_from_lsn,
            Lsn::new(101)
        );
    }

    #[test]
    fn redo_plan_replays_only_manifest_range_and_complete_transactions() {
        let incomplete_tx = TransactionId::new(21);
        let committed_tx = TransactionId::new(22);
        let mut wal = InMemoryWal::new();

        wal.append_tx_begin(incomplete_tx).unwrap();
        let incomplete_row_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(incomplete_tx), b"incomplete")
            .unwrap();
        wal.append_tx_begin(committed_tx).unwrap();
        let committed_row_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(committed_tx), b"complete")
            .unwrap();
        wal.append_tx_commit(committed_tx).unwrap();
        wal.flush_all().unwrap();

        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: incomplete_row_lsn,
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };
        let durable_records = wal.replay_durable();
        let plan = RecoveryPlan::from_manifest_and_wal(
            &manifest,
            StartupMode::SafeStart,
            &durable_records,
        )
            .unwrap();

        assert!(plan.has_incomplete_transactions());
        assert_eq!(
            plan.incomplete_transactions[0].transaction_id,
            incomplete_tx
        );
        assert_eq!(
            plan.replay_lsns().collect::<Vec<_>>(),
            vec![committed_row_lsn]
        );
        assert_eq!(
            plan.records
                .iter()
                .find(|record| record.lsn == incomplete_row_lsn)
                .unwrap()
                .decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
    }

    #[test]
    fn redo_plan_exports_recovery_trace_with_durable_lsn_correlation() {
        let tx = TransactionId::new(30);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(tx).unwrap();
        let row_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(tx), b"complete")
            .unwrap();
        wal.append_tx_commit(tx).unwrap();
        wal.flush_all().unwrap();

        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: row_lsn,
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };
        let durable_records = wal.replay_durable();
        let plan = RecoveryPlan::from_manifest_and_wal(
            &manifest,
            StartupMode::SafeStart,
            &durable_records,
        )
            .unwrap();
        let trace = plan.observe_recovery_trace(TraceId::new(50));

        let envelope = EventEnvelope::new(
            EventId::new(51),
            EventCorrelation {
                durable_lsn: Some(plan.durable_lsn.get()),
                ..EventCorrelation::empty()
            },
            TraceEvent::RecoveryStartup(trace),
        )
            .expect("recovery trace is correlated to the last durable WAL boundary");

        assert_eq!(
            envelope.correlation.durable_lsn,
            Some(plan.durable_lsn.get())
        );
    }

    #[test]
    fn redo_plan_rejects_skipped_lsn_after_required_start() {
        let transaction_id = TransactionId::new(31);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(1),
                None,
                Some(transaction_id),
                Vec::new(),
            )
                .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::RowInsert,
                Lsn::new(3),
                Some(Lsn::new(1)),
                Some(transaction_id),
                b"gap".to_vec(),
            )
                .unwrap(),
        ];
        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };

        assert_eq!(
            RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn redo_plan_rejects_duplicate_lsn_after_required_start() {
        let transaction_id = TransactionId::new(32);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(1),
                None,
                Some(transaction_id),
                Vec::new(),
            )
                .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(1),
                None,
                Some(transaction_id),
                Vec::new(),
            )
                .unwrap(),
        ];
        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };

        assert_eq!(
            RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }

    #[test]
    fn redo_plan_rejects_previous_lsn_mismatch_after_required_start() {
        let transaction_id = TransactionId::new(33);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(1),
                None,
                Some(transaction_id),
                Vec::new(),
            )
                .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(2),
                None,
                Some(transaction_id),
                Vec::new(),
            )
                .unwrap(),
        ];
        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };

        assert_eq!(
            RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );
    }
}
