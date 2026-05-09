use super::super::{BackupListEntry, BackupState};
use crate::error::cli_error;
use andromeda_backup::{BackupArtifactManifestRecord, BackupId, FileBackedBackupArtifactStore};
use andromeda_core::AndromedaResult;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct ValidatedBackupArtifact {
    store: FileBackedBackupArtifactStore,
    pub(super) record: BackupArtifactManifestRecord,
}

impl ValidatedBackupArtifact {
    pub(super) fn artifact_root(&self, backup_id: u64) -> String {
        self.store
            .backup_dir(BackupId::new(backup_id))
            .display()
            .to_string()
    }
}

pub(super) fn validate_existing_backup_artifact(
    artifact_dir: &str,
    backup_id: u64,
    failure_context: &str,
) -> AndromedaResult<ValidatedBackupArtifact> {
    let store = open_existing_backup_store(artifact_dir)?;
    let record = validate_store_artifact(&store, backup_id, failure_context)?;
    Ok(ValidatedBackupArtifact { store, record })
}

pub(super) fn list_file_backed_backup_entries(
    artifact_dir: &str,
) -> AndromedaResult<Vec<BackupListEntry>> {
    let store = open_existing_backup_store(artifact_dir)?;
    let mut entries = Vec::new();
    for (backup_id, path) in sorted_backup_artifact_dirs(&store)? {
        let record = validate_store_artifact(
            &store,
            backup_id,
            "failed to validate listed backup artifact",
        )?;
        entries.push(BackupListEntry {
            backup_id,
            state: BackupState::Completed,
            size_bytes: artifact_total_bytes(&record),
            created_timestamp: record.manifest.created_epoch,
            base_lsn: record.manifest.wal_archive.start.get(),
            end_lsn: record.manifest.wal_archive.end_inclusive.get(),
            artifact_root: Some(path.display().to_string()),
        });
    }

    entries.sort_by(|left, right| {
        right
            .created_timestamp
            .cmp(&left.created_timestamp)
            .then_with(|| right.backup_id.cmp(&left.backup_id))
    });
    Ok(entries)
}

pub(super) fn artifact_total_bytes(record: &BackupArtifactManifestRecord) -> u64 {
    record
        .artifact_set
        .cold_snapshot
        .artifact
        .byte_len
        .saturating_add(record.wal_archive_evidence.total_bytes)
}

fn open_existing_backup_store(path: &str) -> AndromedaResult<FileBackedBackupArtifactStore> {
    FileBackedBackupArtifactStore::open_existing(path).map_err(|error| {
        cli_error(format!(
            "failed to open backup artifact directory `{path}`: {}",
            error.message()
        ))
    })
}

fn validate_store_artifact(
    store: &FileBackedBackupArtifactStore,
    backup_id: u64,
    failure_context: &str,
) -> AndromedaResult<BackupArtifactManifestRecord> {
    store
        .validate_artifact_directory(BackupId::new(backup_id))
        .map_err(|error| {
            cli_error(format!(
                "{failure_context} {backup_id}: {}",
                error.message()
            ))
        })
}

fn sorted_backup_artifact_dirs(
    store: &FileBackedBackupArtifactStore,
) -> AndromedaResult<Vec<(u64, PathBuf)>> {
    let mut dirs = Vec::new();
    let read_dir = fs::read_dir(store.root()).map_err(|error| {
        cli_error(format!(
            "failed to list backup artifact directory `{}`: {error}",
            store.root().display()
        ))
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|error| {
            cli_error(format!(
                "failed to read backup artifact directory entry `{}`: {error}",
                store.root().display()
            ))
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(backup_id) = backup_id_from_dir_name(&path) else {
            continue;
        };
        dirs.push((backup_id, path));
    }

    dirs.sort_by(|(left_id, left_path), (right_id, right_path)| {
        left_id
            .cmp(right_id)
            .then_with(|| left_path.cmp(right_path))
    });
    Ok(dirs)
}

fn backup_id_from_dir_name(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let hex = name.strip_prefix("backup-")?;
    u64::from_str_radix(hex, 16).ok().filter(|id| *id != 0)
}
