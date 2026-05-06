use std::path::Path;
use std::time::Instant;

use andromeda_core::TransactionId;
use andromeda_storage::{
    DatabaseManifest, FileWal, Lsn, StartupMode, WalRecordKind, recover_from_file_wal,
};

use crate::{
    BenchmarkError,
    harness::{BenchmarkTempFile as BenchTempWalFile, elapsed_micros},
};

pub const WAL_APPEND_FILE_WORKLOAD_ID: &str = "wal-append-file-smoke";
pub const WAL_APPEND_FILE_HARNESS_SOURCE: &str = "file-wal-append-flush";
pub const WAL_APPEND_FILE_HARNESS_NAME: &str = "FileWal::append_payload+flush_through";

pub const RECOVERY_REPLAY_WAL_WORKLOAD_ID: &str = "recovery-replay-wal-smoke";
pub const RECOVERY_REPLAY_WAL_HARNESS_SOURCE: &str = "file-wal-recovery-scan";
pub const RECOVERY_REPLAY_WAL_HARNESS_NAME: &str = "recover_from_file_wal";

const FIRST_TRANSACTION_ID: u64 = 50_000;
const EXPECTED_RECORDS_PER_TRANSACTION: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalAppendFileSmokeBenchmark {
    pub latencies_us: Vec<u64>,
    pub appended_records: usize,
    pub durable_lsn: Lsn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReplayWalSmokeBenchmark {
    pub latencies_us: Vec<u64>,
    pub replay_records: usize,
    pub recovered_transaction_id_floor: u64,
}

pub fn run_wal_append_file_smoke_benchmark(
    samples: u32,
) -> Result<WalAppendFileSmokeBenchmark, BenchmarkError> {
    if samples == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }

    execute_wal_append_file_smoke(samples).map_err(|_| BenchmarkError::HarnessFailed)
}

pub fn run_recovery_replay_wal_smoke_benchmark(
    samples: u32,
) -> Result<RecoveryReplayWalSmokeBenchmark, BenchmarkError> {
    if samples == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }

    execute_recovery_replay_wal_smoke(samples).map_err(|_| BenchmarkError::HarnessFailed)
}

fn execute_wal_append_file_smoke(
    samples: u32,
) -> Result<WalAppendFileSmokeBenchmark, WalFileHarnessFailure> {
    let temp =
        BenchTempWalFile::new("andromeda-bench-wal-append", "wal").map_err(harness_failed)?;
    let mut wal = FileWal::open(temp.path()).map_err(harness_failed)?;
    let mut latencies_us = Vec::with_capacity(samples as usize);

    for sample in 0..samples {
        let transaction_id = transaction_id(sample);
        let started = Instant::now();
        wal.append_tx_begin(transaction_id)
            .map_err(harness_failed)?;
        wal.append_payload(
            WalRecordKind::RowInsert,
            Some(transaction_id),
            row_payload(sample),
        )
        .map_err(harness_failed)?;
        let commit_lsn = wal
            .append_tx_commit(transaction_id)
            .map_err(harness_failed)?;
        wal.flush_through(commit_lsn).map_err(harness_failed)?;
        latencies_us.push(elapsed_micros(started));
    }

    let reopened = FileWal::open(temp.path()).map_err(harness_failed)?;
    let appended_records = samples as usize * EXPECTED_RECORDS_PER_TRANSACTION;
    if reopened.len() != appended_records
        || reopened.durable_lsn() != Lsn::new(appended_records as u64)
    {
        return Err(WalFileHarnessFailure);
    }

    Ok(WalAppendFileSmokeBenchmark {
        latencies_us,
        appended_records,
        durable_lsn: reopened.durable_lsn(),
    })
}

fn execute_recovery_replay_wal_smoke(
    samples: u32,
) -> Result<RecoveryReplayWalSmokeBenchmark, WalFileHarnessFailure> {
    let temp = BenchTempWalFile::new("andromeda-bench-recovery-replay-wal", "wal")
        .map_err(harness_failed)?;
    build_recovery_wal_fixture(temp.path())?;

    let manifest = recovery_manifest();
    let mut latencies_us = Vec::with_capacity(samples as usize);
    let mut replay_records = None;
    let mut recovered_transaction_id_floor = 0;

    for _ in 0..samples {
        let started = Instant::now();
        let plan = recover_from_file_wal(&manifest, StartupMode::SafeStart, temp.path())
            .map_err(harness_failed)?;
        latencies_us.push(elapsed_micros(started));

        let current_replay_records = plan.replay_lsns().count();
        if current_replay_records == 0 {
            return Err(WalFileHarnessFailure);
        }
        if replay_records.is_some_and(|expected| expected != current_replay_records) {
            return Err(WalFileHarnessFailure);
        }
        replay_records = Some(current_replay_records);
        recovered_transaction_id_floor = plan.recovered_transaction_id_floor();
    }

    Ok(RecoveryReplayWalSmokeBenchmark {
        latencies_us,
        replay_records: replay_records.ok_or(WalFileHarnessFailure)?,
        recovered_transaction_id_floor,
    })
}

fn build_recovery_wal_fixture(path: &Path) -> Result<(), WalFileHarnessFailure> {
    let committed = TransactionId::new(FIRST_TRANSACTION_ID);
    let rolled_back = TransactionId::new(FIRST_TRANSACTION_ID + 1);
    let incomplete = TransactionId::new(FIRST_TRANSACTION_ID + 2);
    let mut wal = FileWal::open(path).map_err(harness_failed)?;

    wal.append_tx_begin(committed).map_err(harness_failed)?;
    wal.append_payload(WalRecordKind::RowInsert, Some(committed), b"committed-row")
        .map_err(harness_failed)?;
    wal.append_tx_commit(committed).map_err(harness_failed)?;

    wal.append_tx_begin(rolled_back).map_err(harness_failed)?;
    wal.append_payload(
        WalRecordKind::RowUpdate,
        Some(rolled_back),
        b"rolled-back-row",
    )
    .map_err(harness_failed)?;
    wal.append_tx_rollback(rolled_back)
        .map_err(harness_failed)?;

    wal.append_tx_begin(incomplete).map_err(harness_failed)?;
    wal.append_payload(
        WalRecordKind::RowDelete,
        Some(incomplete),
        b"incomplete-row",
    )
    .map_err(harness_failed)?;
    wal.flush_all().map_err(harness_failed)?;
    Ok(())
}

fn recovery_manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

fn transaction_id(sample: u32) -> TransactionId {
    TransactionId::new(FIRST_TRANSACTION_ID + u64::from(sample))
}

fn row_payload(sample: u32) -> Vec<u8> {
    format!("andromeda-bench-row-{sample:04}").into_bytes()
}

#[derive(Debug)]
struct WalFileHarnessFailure;

fn harness_failed<E>(_error: E) -> WalFileHarnessFailure {
    WalFileHarnessFailure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_append_file_smoke_appends_flushes_and_reopens() {
        let result = run_wal_append_file_smoke_benchmark(3).unwrap();

        assert_eq!(result.latencies_us.len(), 3);
        assert_eq!(result.appended_records, 9);
        assert_eq!(result.durable_lsn, Lsn::new(9));
        assert!(result.latencies_us.iter().all(|latency| *latency >= 1));
    }

    #[test]
    fn wal_append_file_smoke_rejects_zero_samples() {
        assert_eq!(
            run_wal_append_file_smoke_benchmark(0).unwrap_err(),
            BenchmarkError::InsufficientSamplesForStatistics
        );
    }

    #[test]
    fn recovery_replay_wal_smoke_scans_and_builds_redo_plan() {
        let result = run_recovery_replay_wal_smoke_benchmark(2).unwrap();

        assert_eq!(result.latencies_us.len(), 2);
        assert_eq!(result.replay_records, 1);
        assert_eq!(
            result.recovered_transaction_id_floor,
            FIRST_TRANSACTION_ID + 2
        );
        assert!(result.latencies_us.iter().all(|latency| *latency >= 1));
    }

    #[test]
    fn recovery_replay_wal_smoke_rejects_zero_samples() {
        assert_eq!(
            run_recovery_replay_wal_smoke_benchmark(0).unwrap_err(),
            BenchmarkError::InsufficientSamplesForStatistics
        );
    }
}
