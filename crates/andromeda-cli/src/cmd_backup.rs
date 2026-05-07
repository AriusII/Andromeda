mod execution;
mod output;
mod parse;

#[cfg(test)]
mod tests;

pub(crate) use execution::run_backup_command;

/// Backup status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupState {
    ContractPreview,
    Pending,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for BackupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupState::ContractPreview => write!(f, "contract_preview"),
            BackupState::Pending => write!(f, "pending"),
            BackupState::Running => write!(f, "running"),
            BackupState::Completed => write!(f, "completed"),
            BackupState::Failed => write!(f, "failed"),
        }
    }
}

impl BackupState {
    pub(super) const ALL: [Self; 4] = [Self::Pending, Self::Running, Self::Completed, Self::Failed];
}

/// Backup status report.
#[derive(Debug, Clone)]
struct BackupStatusReport {
    backup_id: u64,
    state: BackupState,
    progress_percent: u32,
    bytes_processed: u64,
    estimated_total_bytes: u64,
    start_time: u64,
    elapsed_seconds: u64,
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
    artifact_root: Option<String>,
    manifest_path: Option<String>,
    wal_segment_count: usize,
    base_lsn: u64,
    end_lsn: u64,
    message: String,
}

/// Backup list entry.
#[derive(Debug, Clone)]
struct BackupListEntry {
    backup_id: u64,
    state: BackupState,
    size_bytes: u64,
    created_timestamp: u64,
    base_lsn: u64,
    end_lsn: u64,
    artifact_root: Option<String>,
}

/// Backup start outcome.
#[derive(Debug, Clone)]
struct BackupStartOutcome {
    backup_id: Option<u64>,
    backup_type: String,
    destination: Option<String>,
    artifact_dir: Option<String>,
    manifest_path: Option<String>,
    snapshot_path: Option<String>,
    wal_segment_paths: Vec<String>,
    base_lsn: Option<u64>,
    end_lsn: Option<u64>,
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
    dry_run: bool,
    would_start: bool,
    message: String,
}

#[derive(Debug, Clone)]
struct BackupVerifyOutcome {
    backup_id: u64,
    artifact_dir: String,
    manifest_path: String,
    snapshot_path: String,
    wal_segment_count: usize,
    snapshot_bytes: u64,
    wal_archive_bytes: u64,
    base_lsn: u64,
    end_lsn: u64,
    message: String,
}

#[derive(Debug, Clone)]
struct BackupCancelOutcome {
    backup_id: u64,
    artifact_dir: Option<String>,
    contract_preview: bool,
    durable_backend: bool,
    dry_run: bool,
    cancelled: bool,
    message: String,
}
