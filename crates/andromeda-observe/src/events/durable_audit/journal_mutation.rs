use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use super::{
    DurableAuditFailureKind, DurableAuditRecordIdentity, DurableAuditSinkResult, sink_failure,
};

pub(super) struct DurableAuditMutationLock {
    path: PathBuf,
}

impl Drop for DurableAuditMutationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn acquire_mutation_lock(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<DurableAuditMutationLock> {
    let lock_path = journal_sidecar_path(path, ".lock");
    let mut lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            let reason = if error.kind() == std::io::ErrorKind::AlreadyExists {
                "durable audit journal mutation lock is already held".to_string()
            } else {
                format!("failed to acquire durable audit journal mutation lock: {error}")
            };
            sink_failure(failure_kind, identity, reason)
        })?;
    lock.write_all(b"andromeda-durable-audit-mutation-lock-v1\n")
        .map_err(|error| {
            let _ = fs::remove_file(&lock_path);
            sink_failure(
                failure_kind,
                identity,
                format!("failed to write durable audit journal mutation lock: {error}"),
            )
        })?;
    lock.sync_all().map_err(|error| {
        let _ = fs::remove_file(&lock_path);
        sink_failure(
            failure_kind,
            identity,
            format!("failed to flush durable audit journal mutation lock: {error}"),
        )
    })?;
    Ok(DurableAuditMutationLock { path: lock_path })
}

pub(super) fn publish_compacted_journal(
    tmp_path: &Path,
    path: &Path,
) -> DurableAuditSinkResult<()> {
    match fs::rename(tmp_path, path) {
        Ok(()) => Ok(()),
        Err(error) if path.exists() => {
            fs::remove_file(path).map_err(|remove_error| {
                sink_failure(
                    DurableAuditFailureKind::RetentionRejected,
                    None,
                    format!(
                        "failed to replace durable audit journal during compaction after rename error {error}: {remove_error}"
                    ),
                )
            })?;
            fs::rename(tmp_path, path).map_err(|rename_error| {
                sink_failure(
                    DurableAuditFailureKind::RetentionRejected,
                    None,
                    format!("failed to publish durable audit compacted journal: {rename_error}"),
                )
            })
        }
        Err(error) => Err(sink_failure(
            DurableAuditFailureKind::RetentionRejected,
            None,
            format!("failed to publish durable audit compacted journal: {error}"),
        )),
    }?;
    sync_parent_directory(path, DurableAuditFailureKind::RetentionRejected, None)
}

pub(super) fn journal_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

#[cfg(unix)]
fn sync_parent_directory(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = fs::File::open(parent).map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to open durable audit parent directory for sync: {error}"),
        )
    })?;
    directory.sync_all().map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to sync durable audit parent directory: {error}"),
        )
    })
}

#[cfg(not(unix))]
fn sync_parent_directory(
    _path: &Path,
    _failure_kind: DurableAuditFailureKind,
    _identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    Ok(())
}
