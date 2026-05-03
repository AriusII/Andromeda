use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::TraceId;

use crate::{
    DatabaseManifest, IncompleteDurableTransaction, Lsn, WalRecord, WalRecordKind,
    incomplete_transactions_from_records,
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

    pub fn build_redo_plan(
        self,
        durable_records: &[WalRecord],
    ) -> AndromedaResult<ConceptualRedoPlan> {
        for record in durable_records {
            record.validate()?;
        }
        validate_wal_coverage(self.redo_from_lsn, durable_records)?;

        let durable_lsn = durable_records
            .iter()
            .map(|record| record.header.lsn)
            .max()
            .unwrap_or_default();
        let incomplete_transactions = incomplete_transactions_from_records(durable_records);

        let records = durable_records
            .iter()
            .map(|record| RedoRecordPlan {
                lsn: record.header.lsn,
                kind: record.header.kind,
                transaction_id: record.header.transaction_id,
                decision: redo_decision_for_record(self, record, &incomplete_transactions),
            })
            .collect();

        Ok(ConceptualRedoPlan {
            startup_mode: self.startup_mode,
            mounted_snapshot_id: self.mounted_snapshot_id,
            redo_from_lsn: self.redo_from_lsn,
            durable_lsn,
            discard_incomplete_transactions: self.discard_incomplete_transactions,
            incomplete_transactions,
            records,
        })
    }
}

fn validate_wal_coverage(redo_from_lsn: Lsn, durable_records: &[WalRecord]) -> AndromedaResult<()> {
    if redo_from_lsn.is_zero() {
        return Ok(());
    }

    let mut expected_lsn = None;
    let mut previous_in_redo_range = None;
    let mut saw_redo_start = false;

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
            saw_redo_start = true;
        }

        previous_in_redo_range = Some(record.header.lsn);
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

    Ok(())
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedoRecordDecision {
    Replay,
    SkipBeforeRedoStart,
    SkipIncompleteTransaction,
    SkipNonRedoRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedoRecordPlan {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<andromeda_core::TransactionId>,
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
    pub incomplete_transactions: Vec<IncompleteDurableTransaction>,
    pub records: Vec<RedoRecordPlan>,
}

impl ConceptualRedoPlan {
    pub fn replay_lsns(&self) -> impl Iterator<Item = Lsn> + '_ {
        self.records
            .iter()
            .filter(|record| record.should_replay())
            .map(|record| record.lsn)
    }

    pub fn has_incomplete_transactions(&self) -> bool {
        !self.incomplete_transactions.is_empty()
    }
}

fn redo_decision_for_record(
    plan: RecoveryPlan,
    record: &WalRecord,
    incomplete_transactions: &[IncompleteDurableTransaction],
) -> RedoRecordDecision {
    if record.header.lsn < plan.redo_from_lsn {
        return RedoRecordDecision::SkipBeforeRedoStart;
    }

    if plan.discard_incomplete_transactions
        && record.header.transaction_id.is_some_and(|transaction_id| {
            incomplete_transactions
                .iter()
                .any(|incomplete| incomplete.transaction_id == transaction_id)
        })
    {
        return RedoRecordDecision::SkipIncompleteTransaction;
    }

    if !record.header.kind.is_redo_relevant() {
        return RedoRecordDecision::SkipNonRedoRecord;
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
