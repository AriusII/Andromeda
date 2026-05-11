use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    BackupValidationError,
    error::{BackupResult, backup_error},
    types::BackupId,
};

pub(super) const MANIFEST_FILE_NAME: &str = "backup.manifest";
pub(super) const SNAPSHOT_FILE_NAME: &str = "snapshot.bin";
pub(super) const CATALOG_FILE_NAME: &str = "catalog.bin";
pub(super) const AUDIT_LEDGER_FILE_NAME: &str = "audit-ledger.bin";
pub(super) const WAL_DIRECTORY_NAME: &str = "wal";

const BACKUP_ARTIFACT_MANIFEST_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn write_file_atomically(path: &Path, bytes: &[u8]) -> BackupResult<()> {
    if bytes.is_empty() {
        return Err(backup_error("backup artifact file bytes must not be empty"));
    }
    if path.exists() {
        return Err(backup_error("backup artifact file already exists"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| backup_error("backup artifact path must have a parent directory"))?;
    fs::create_dir_all(parent).map_err(|err| io_error("create backup artifact directory", err))?;

    let tmp_path = path.with_extension("tmp");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path)
            .map_err(|err| io_error("remove stale backup artifact temp file", err))?;
    }
    let mut tmp =
        File::create(&tmp_path).map_err(|err| io_error("create backup artifact temp file", err))?;
    tmp.write_all(bytes)
        .map_err(|err| io_error("write backup artifact temp file", err))?;
    tmp.sync_all()
        .map_err(|err| io_error("sync backup artifact temp file", err))?;
    drop(tmp);
    fs::rename(&tmp_path, path).map_err(|err| io_error("rename backup artifact temp file", err))?;
    Ok(())
}

pub(super) fn read_manifest_file(path: &Path) -> BackupResult<Vec<u8>> {
    let metadata =
        fs::metadata(path).map_err(|err| io_error("stat backup artifact manifest", err))?;
    if metadata.len() > BACKUP_ARTIFACT_MANIFEST_MAX_FILE_BYTES {
        return Err(backup_error(
            "backup artifact manifest file exceeds bounded read limit",
        ));
    }
    fs::read(path).map_err(|err| io_error("read backup artifact manifest", err))
}

pub(super) fn read_file_with_expected_len(
    path: &Path,
    expected_len: u64,
    label: &'static str,
) -> BackupResult<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|err| io_error(format!("stat {label}"), err))?;
    if metadata.len() != expected_len {
        return Err(backup_error(format!("{label} byte length mismatch")));
    }
    let bytes = fs::read(path).map_err(|err| io_error(format!("read {label}"), err))?;
    let actual_len = u64::try_from(bytes.len())
        .map_err(|_| backup_error(format!("{label} length exceeds u64")))?;
    if actual_len != expected_len {
        return Err(backup_error(format!("{label} byte length mismatch")));
    }
    Ok(bytes)
}

pub(super) fn backup_dir_name(backup_id: BackupId) -> String {
    format!("backup-{:016x}", backup_id.get())
}

pub(super) fn wal_segment_path(
    backup_dir: &Path,
    sequence_index: usize,
    segment_id: u64,
) -> PathBuf {
    backup_dir
        .join(WAL_DIRECTORY_NAME)
        .join(format!("segment-{sequence_index:06}-{segment_id:016x}.wal"))
}

pub(super) fn sync_directory_best_effort(path: &Path) {
    if let Ok(file) = File::open(path) {
        let _ = file.sync_all();
    }
}

pub(super) fn io_error(action: impl Into<String>, err: std::io::Error) -> BackupValidationError {
    backup_error(format!("{}: {err}", action.into()))
}
