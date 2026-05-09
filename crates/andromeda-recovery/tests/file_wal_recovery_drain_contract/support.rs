use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::recover_from_file_wal;
use andromeda_recovery::{RedoRecordDecision, StartupMode};
use andromeda_types::TransactionId;
use andromeda_wal::FileWal;
use andromeda_wal::Lsn;
use andromeda_wal::WalRecordKind;
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) struct TempWalPath {
    pub(crate) path: PathBuf,
}

impl TempWalPath {
    pub(crate) fn new(name: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "andromeda-recovery-{name}-{}-{suffix}.wal",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self { path }
    }
}

impl Drop for TempWalPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

pub(crate) fn planned_decision(path: &Path, lsn: Lsn) -> RedoRecordDecision {
    recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, path)
        .unwrap()
        .records
        .into_iter()
        .find(|record| record.lsn == lsn)
        .expect("planned record should exist")
        .decision
}

pub(crate) fn write_two_committed_transactions(path: &Path) -> Lsn {
    let first_transaction = TransactionId::new(704);
    let second_transaction = TransactionId::new(705);
    let mut wal = FileWal::open(path).unwrap();

    wal.append_tx_begin(first_transaction).unwrap();
    let first_row_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(first_transaction), b"first")
        .unwrap();
    wal.append_tx_commit(first_transaction).unwrap();
    wal.append_tx_begin(second_transaction).unwrap();
    wal.append_payload(
        WalRecordKind::RowUpdate,
        Some(second_transaction),
        b"second",
    )
    .unwrap();
    wal.append_tx_commit(second_transaction).unwrap();
    wal.flush_all().unwrap();

    first_row_lsn
}
