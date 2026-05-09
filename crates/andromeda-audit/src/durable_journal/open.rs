use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

use super::{
    DurableAuditFailureKind, DurableAuditReplayQuery, DurableAuditSinkResult,
    journal_format::replay_durable_audit_journal, sink_failure,
};

pub(crate) fn open_sink(path: impl AsRef<Path>) -> DurableAuditSinkResult<PathBuf> {
    let path = path.as_ref().to_path_buf();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                None,
                format!("failed to create durable audit journal directory: {error}"),
            )
        })?;
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                None,
                format!("failed to open durable audit journal: {error}"),
            )
        })?;

    replay_durable_audit_journal(&path, &DurableAuditReplayQuery::all())?;
    Ok(path)
}
