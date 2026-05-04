mod coverage;
mod planning;
mod trace;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use coverage::WalCoverageEvidence;
pub use planning::{
    ConceptualRedoPlan, RecoveryPlan, RedoRecordDecision, RedoRecordPlan, StartupMode,
};
pub use trace::RecoveryTrace;

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        encode_wal_record, scan_wal_records, DatabaseManifest, DurableTransactionState,
        InMemoryWal, Lsn, WalRecord, WalRecordKind, WalScanStopReason,
    };
    use andromeda_core::{AndromedaErrorKind, TransactionId};
    use andromeda_observe::{EventCorrelation, EventEnvelope, EventId, TraceEvent, TraceId};

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
    fn redo_plan_skips_transaction_when_commit_is_not_durable() {
        let transaction_id = TransactionId::new(23);
        let mut wal = InMemoryWal::new();

        wal.append_tx_begin(transaction_id).unwrap();
        let row_lsn = wal
            .append_payload(
                WalRecordKind::RowDelete,
                Some(transaction_id),
                b"not-committed",
            )
            .unwrap();
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
        wal.flush_through(row_lsn).unwrap();

        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
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

        assert_eq!(commit_lsn, Lsn::new(3));
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
        assert_eq!(plan.incomplete_transactions.len(), 1);
        assert_eq!(
            plan.records
                .iter()
                .find(|record| record.lsn == row_lsn)
                .unwrap()
                .decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
    }

    #[test]
    fn redo_plan_skips_transaction_with_commit_but_missing_begin_evidence() {
        let transaction_id = TransactionId::new(24);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::RowUpdate,
                Lsn::new(1),
                None,
                Some(transaction_id),
                b"missing-begin".to_vec(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(2),
                Some(Lsn::new(1)),
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

        let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
            .unwrap();

        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());
        assert_eq!(
            plan.transaction_evidence[0].state,
            DurableTransactionState::Incomplete
        );
        assert_eq!(
            plan.records
                .iter()
                .find(|record| record.lsn == Lsn::new(1))
                .unwrap()
                .decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
    }

    #[test]
    fn redo_plan_from_truncated_wal_scan_replays_only_complete_prefix_transactions() {
        let committed_tx = TransactionId::new(25);
        let tail_tx = TransactionId::new(26);
        let records = vec![
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(1),
                None,
                Some(committed_tx),
                Vec::new(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::RowInsert,
                Lsn::new(2),
                Some(Lsn::new(1)),
                Some(committed_tx),
                b"complete".to_vec(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(3),
                Some(Lsn::new(2)),
                Some(committed_tx),
                Vec::new(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxBegin,
                Lsn::new(4),
                Some(Lsn::new(3)),
                Some(tail_tx),
                Vec::new(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::RowUpdate,
                Lsn::new(5),
                Some(Lsn::new(4)),
                Some(tail_tx),
                b"tail".to_vec(),
            )
            .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::TxCommit,
                Lsn::new(6),
                Some(Lsn::new(5)),
                Some(tail_tx),
                Vec::new(),
            )
            .unwrap(),
        ];
        let mut encoded = Vec::new();
        for record in &records {
            encoded.extend(encode_wal_record(record).unwrap());
        }
        encoded.truncate(encoded.len() - 1);
        let scan = scan_wal_records(&encoded);
        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };

        let plan =
            RecoveryPlan::from_manifest_and_wal_scan(&manifest, StartupMode::SafeStart, &scan)
                .unwrap();

        assert_eq!(
            plan.wal_scan_stop().unwrap().reason,
            WalScanStopReason::TruncatedHeader
        );
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(plan.incomplete_transactions.len(), 1);
        assert_eq!(plan.incomplete_transactions[0].transaction_id, tail_tx);
        assert_eq!(
            plan.records
                .iter()
                .find(|record| record.lsn == Lsn::new(5))
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
