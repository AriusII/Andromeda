//! Canonical on-disk file-backed WAL implementation.
//!
//! Owns `FileWal`, `FileWalHeader`, `FILE_WAL_*` byte-format constants, and the
//! v0 recovery report. The [`crate::write_ahead_log::file`] facade re-exports
//! these names; do not redefine them. The on-disk header and frame layout are
//! load-bearing for crash recovery and must not change without bumping
//! `WAL_FORMAT_VERSION`.
use andromeda_core::{AndromedaError, AndromedaErrorKind};

mod format;
mod header;
mod recovery;
mod report;
mod scan;
mod wal;

pub use header::{FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWalHeader};
pub use recovery::{
    FileWalDiskScan, FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
    FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord, FileWalRecoveryReportV0,
    FileWalStartupRecoveryV0, plan_file_wal_startup_recovery_v0, recover_from_file_wal,
    report_file_wal_recovery_v0, scan_file_wal,
};
pub use wal::FileWal;

fn io_error(action: &str, error: std::io::Error) -> AndromedaError {
    storage_error(format!("{action}: {error}"))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::format::write_file_wal_header;
    use super::*;
    use crate::{
        DatabaseManifest, Lsn, ObservedBoundary, RedoRecordDecision, StartupMode,
        StartupRejectionReason, WalRecord, WalRecordKind, WalScanStopReason, encode_wal_record,
    };
    use andromeda_core::TransactionId;
    use std::fs::{File, OpenOptions, metadata, remove_file};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn test_wal_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "andromeda-storage-{test_name}-{}.wal",
            std::process::id()
        ))
    }

    fn recovery_manifest() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(1),
            required_wal_start_lsn: Lsn::new(1),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        }
    }

    fn write_raw_wal_file(path: &Path, records: &[WalRecord]) {
        remove_file(path).ok();
        let encoded = encode_records(records);
        let durable_lsn = records
            .last()
            .map(|record| record.header.lsn)
            .unwrap_or(Lsn::ZERO);
        let header = FileWalHeader::new(durable_lsn, encoded.len() as u64, records.len() as u64);
        let mut file = File::create(path).unwrap();
        write_file_wal_header(&mut file, &header).unwrap();
        file.write_all(&encoded).unwrap();
        file.sync_all().unwrap();
    }

    fn encode_records(records: &[WalRecord]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for record in records {
            encoded.extend(encode_wal_record(record).unwrap());
        }
        encoded
    }

    #[test]
    fn file_wal_open_truncates_unflushed_physical_tail_before_replay() {
        let path = test_wal_path("truncates-unflushed-tail");
        remove_file(&path).ok();
        let committed_tx = TransactionId::new(101);
        let unflushed_tx = TransactionId::new(102);

        {
            let mut wal = FileWal::open(&path).unwrap();
            wal.append_tx_begin(committed_tx).unwrap();
            let committed_row_lsn = wal
                .append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"complete")
                .unwrap();
            wal.append_tx_commit(committed_tx).unwrap();
            wal.flush_all().unwrap();

            wal.append_tx_begin(unflushed_tx).unwrap();
            wal.append_payload(WalRecordKind::RowUpdate, Some(unflushed_tx), b"not-durable")
                .unwrap();
            assert_eq!(committed_row_lsn, Lsn::new(2));
            assert_eq!(wal.last_lsn(), Some(Lsn::new(5)));
            assert_eq!(wal.durable_lsn(), Lsn::new(3));
        }

        let wal = FileWal::open(&path).unwrap();
        let expected_len = format::FILE_WAL_DATA_OFFSET + wal.durable_bytes();
        assert_eq!(wal.last_lsn(), Some(Lsn::new(3)));
        assert_eq!(wal.durable_lsn(), Lsn::new(3));
        assert_eq!(metadata(&path).unwrap().len(), expected_len);
        drop(wal);

        let plan =
            recover_from_file_wal(&recovery_manifest(), StartupMode::SafeStart, &path).unwrap();
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert!(
            !plan
                .incomplete_transactions
                .iter()
                .any(|transaction| transaction.transaction_id == unflushed_tx)
        );

        remove_file(&path).ok();
    }

    #[test]
    fn file_wal_startup_recovery_seam_audits_clean_prefix_and_allocator_floor() {
        let path = test_wal_path("startup-clean-prefix");
        remove_file(&path).ok();
        let committed_tx = TransactionId::new(201);
        let rolled_back_tx = TransactionId::new(202);
        let incomplete_tx = TransactionId::new(333);

        {
            let mut wal = FileWal::open(&path).unwrap();
            wal.append_tx_begin(committed_tx).unwrap();
            let committed_row_lsn = wal
                .append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"committed")
                .unwrap();
            wal.append_tx_commit(committed_tx).unwrap();
            wal.append_tx_begin(rolled_back_tx).unwrap();
            let rolled_back_row_lsn = wal
                .append_payload(
                    WalRecordKind::RowUpdate,
                    Some(rolled_back_tx),
                    b"rolled-back",
                )
                .unwrap();
            wal.append_tx_rollback(rolled_back_tx).unwrap();
            wal.append_tx_begin(incomplete_tx).unwrap();
            let incomplete_row_lsn = wal
                .append_payload(WalRecordKind::RowDelete, Some(incomplete_tx), b"incomplete")
                .unwrap();
            wal.flush_all().unwrap();

            assert_eq!(committed_row_lsn, Lsn::new(2));
            assert_eq!(rolled_back_row_lsn, Lsn::new(5));
            assert_eq!(incomplete_row_lsn, Lsn::new(8));
        }

        let startup = plan_file_wal_startup_recovery_v0(
            &recovery_manifest(),
            StartupMode::SafeStart,
            &path,
            false,
        )
        .unwrap();
        let redo = startup
            .redo_plan
            .as_ref()
            .expect("clean safe-start replays");

        assert!(startup.decision.is_accepted());
        assert!(startup.replay_allowed());
        assert_eq!(
            startup.evidence.observed_boundary(),
            ObservedBoundary::Clean
        );
        assert_eq!(redo.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(
            redo.records
                .iter()
                .find(|record| record.lsn == Lsn::new(5))
                .unwrap()
                .decision,
            RedoRecordDecision::SkipRolledBackTransaction
        );
        assert_eq!(
            redo.records
                .iter()
                .find(|record| record.lsn == Lsn::new(8))
                .unwrap()
                .decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );
        assert_eq!(redo.recovered_transaction_id_floor(), incomplete_tx.get());
        assert_eq!(
            startup.transaction_manager_allocator_floor(),
            incomplete_tx.get()
        );
        assert_eq!(
            startup
                .audit_projection(andromeda_observe::TraceId::new(77))
                .last_durable_lsn,
            Lsn::new(8)
        );

        remove_file(&path).ok();
    }

    #[test]
    fn file_wal_startup_recovery_accepts_recoverable_tail_but_ignores_incomplete() {
        let path = test_wal_path("startup-recoverable-tail");
        remove_file(&path).ok();
        let committed_tx = TransactionId::new(204);
        let tail_tx = TransactionId::new(205);

        {
            let mut wal = FileWal::open(&path).unwrap();
            wal.append_tx_begin(committed_tx).unwrap();
            wal.append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"complete")
                .unwrap();
            wal.append_tx_commit(committed_tx).unwrap();
            wal.append_tx_begin(tail_tx).unwrap();
            wal.append_payload(WalRecordKind::RowUpdate, Some(tail_tx), b"tail")
                .unwrap();
            wal.append_tx_commit(tail_tx).unwrap();
            wal.flush_all().unwrap();
        }
        let original_len = metadata(&path).unwrap().len();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(original_len - 1)
            .unwrap();

        let startup = plan_file_wal_startup_recovery_v0(
            &recovery_manifest(),
            StartupMode::SafeStart,
            &path,
            false,
        )
        .unwrap();
        let redo = startup
            .redo_plan
            .as_ref()
            .expect("safe start replays valid prefix before recoverable tail");

        assert!(startup.decision.is_accepted());
        assert_eq!(
            startup.evidence.observed_boundary(),
            ObservedBoundary::RecoverableTail
        );
        assert_eq!(startup.disk_scan.durable_lsn, Lsn::new(5));
        assert_eq!(redo.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(redo.incomplete_transactions.len(), 1);
        assert_eq!(redo.incomplete_transactions[0].transaction_id, tail_tx);
        assert_eq!(startup.transaction_manager_allocator_floor(), tail_tx.get());

        remove_file(&path).ok();
    }

    #[test]
    fn file_wal_startup_recovery_routes_forensic_chain_break_without_redo() {
        let path = test_wal_path("startup-forensic-chain-break");
        let tx = TransactionId::new(206);
        let records = vec![
            WalRecord::from_parts(WalRecordKind::TxBegin, Lsn::new(1), None, Some(tx), b"")
                .unwrap(),
            WalRecord::from_parts(
                WalRecordKind::RowInsert,
                Lsn::new(2),
                None,
                Some(tx),
                b"bad-prev",
            )
            .unwrap(),
        ];
        write_raw_wal_file(&path, &records);

        let safe_start = plan_file_wal_startup_recovery_v0(
            &recovery_manifest(),
            StartupMode::SafeStart,
            &path,
            false,
        )
        .unwrap();
        assert_eq!(
            safe_start.evidence.observed_boundary(),
            ObservedBoundary::ForensicChainBreak
        );
        assert_eq!(
            safe_start.decision.rejection(),
            Some(StartupRejectionReason::ForensicHandlingRequired)
        );
        assert!(safe_start.redo_plan.is_none());

        let forensic_start = plan_file_wal_startup_recovery_v0(
            &recovery_manifest(),
            StartupMode::ForensicStart,
            &path,
            true,
        )
        .unwrap();
        assert!(forensic_start.decision.is_accepted());
        assert!(
            !forensic_start.replay_allowed(),
            "ForensicStart must not mutate truth through redo"
        );
        assert!(forensic_start.redo_plan.is_none());
        assert_eq!(
            forensic_start.transaction_manager_allocator_floor(),
            tx.get()
        );

        remove_file(&path).ok();
    }

    #[test]
    fn file_wal_recovery_skips_transaction_with_truncated_durable_commit() {
        let path = test_wal_path("truncated-durable-commit");
        remove_file(&path).ok();
        let committed_tx = TransactionId::new(103);
        let tail_tx = TransactionId::new(104);

        {
            let mut wal = FileWal::open(&path).unwrap();
            wal.append_tx_begin(committed_tx).unwrap();
            wal.append_payload(WalRecordKind::RowInsert, Some(committed_tx), b"complete")
                .unwrap();
            wal.append_tx_commit(committed_tx).unwrap();
            wal.append_tx_begin(tail_tx).unwrap();
            wal.append_payload(WalRecordKind::RowUpdate, Some(tail_tx), b"tail")
                .unwrap();
            wal.append_tx_commit(tail_tx).unwrap();
            wal.flush_all().unwrap();
            assert_eq!(wal.durable_lsn(), Lsn::new(6));
        }
        let original_len = metadata(&path).unwrap().len();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(original_len - 1)
            .unwrap();

        let report =
            report_file_wal_recovery_v0(&recovery_manifest(), StartupMode::SafeStart, &path)
                .unwrap();

        assert_eq!(
            report.scan_stop.unwrap().reason,
            WalScanStopReason::TruncatedHeader
        );
        assert_eq!(
            report.boundary_kind,
            FileWalRecoveryBoundaryKind::RecoverableTail
        );
        assert_eq!(report.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(report.ignored_transactions.len(), 1);
        assert_eq!(report.ignored_transactions[0].transaction_id, tail_tx);
        assert_eq!(
            report.ignored_transactions[0].reason,
            FileWalRecoveryIgnoredTransactionReason::Incomplete
        );
        assert_eq!(report.ignored_record_count, 1);

        let plan =
            recover_from_file_wal(&recovery_manifest(), StartupMode::SafeStart, &path).unwrap();
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
        assert_eq!(plan.incomplete_transactions.len(), 1);
        assert_eq!(
            plan.records
                .iter()
                .find(|record| record.lsn == Lsn::new(5))
                .unwrap()
                .decision,
            RedoRecordDecision::SkipIncompleteTransaction
        );

        remove_file(&path).ok();
    }
}
