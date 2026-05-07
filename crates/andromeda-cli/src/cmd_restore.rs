mod execution;
mod output;
mod parse;

#[cfg(test)]
mod tests;

pub(crate) use execution::run_restore_command;

/// Restore state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreState {
    ContractPreview,
    Pending,
    ValidatingManifest,
    ReplayingWal,
    Completed,
    Failed,
}

impl std::fmt::Display for RestoreState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreState::ContractPreview => write!(f, "contract_preview"),
            RestoreState::Pending => write!(f, "pending"),
            RestoreState::ValidatingManifest => write!(f, "validating_manifest"),
            RestoreState::ReplayingWal => write!(f, "replaying_wal"),
            RestoreState::Completed => write!(f, "completed"),
            RestoreState::Failed => write!(f, "failed"),
        }
    }
}

impl RestoreState {
    pub(super) const ALL: [Self; 5] = [
        Self::Pending,
        Self::ValidatingManifest,
        Self::ReplayingWal,
        Self::Completed,
        Self::Failed,
    ];
}

/// Restore status report.
#[derive(Debug, Clone)]
struct RestoreStatusReport {
    restore_id: u64,
    state: RestoreState,
    backup_id: u64,
    pitr_target_lsn: Option<u64>,
    progress_percent: u32,
    wal_segments_replayed: u64,
    estimated_total_segments: u64,
    start_time: u64,
    elapsed_seconds: u64,
}

/// Restore start outcome.
#[derive(Debug, Clone)]
struct RestoreStartOutcome {
    restore_id: Option<u64>,
    backup_id: u64,
    artifact_path: String,
    pitr_target_lsn: Option<u64>,
    pitr_policy: Option<String>,
    validation_policy: String,
    preflight_validated: bool,
    replay_segments: Vec<RestoreReplaySegmentOutput>,
    contract_preview: bool,
    durable_backend: bool,
    requires_restore_orchestrator: bool,
    dry_run: bool,
    would_restore: bool,
    message: String,
}

#[derive(Debug, Clone)]
struct RestoreVerifyOutcome {
    backup_id: u64,
    artifact_path: String,
    pitr_target_lsn: u64,
    pitr_policy: Option<String>,
    validation_policy: String,
    source_checkpoint_lsn: u64,
    replay_segments: Vec<RestoreReplaySegmentOutput>,
    message: String,
}

#[derive(Debug, Clone)]
struct RestoreReplaySegmentOutput {
    sequence_index: usize,
    segment_id: u64,
    first_lsn: u64,
    last_lsn: u64,
    contains_pitr_target: bool,
}
