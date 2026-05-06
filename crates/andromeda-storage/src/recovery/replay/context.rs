use std::collections::HashMap;

use crate::{DatabaseManifest, Lsn};

use super::result::{ReplayOutcome, ReplayResult};

/// Replay context for database recovery.
///
/// This context is passed to all replay handlers and carries the state needed
/// to apply WAL records to the database.
pub struct ReplayContext {
    // TECH-DEBT(recovery-replay-context): Replay state is intentionally minimal until
    // heap/index/catalog/MVCC redo handlers stop fail-stopping.
    /// Highest LSN replayed so far in this recovery session.
    pub last_replayed_lsn: Option<Lsn>,

    /// Count of successfully applied records.
    pub applied_count: usize,

    /// Count of skipped records.
    pub skipped_count: usize,

    /// Records that failed to apply.
    pub error_records: Vec<ReplayResult>,

    /// Active manifest anchor after replay.
    pub active_manifest: Option<DatabaseManifest>,

    /// CRC catalog for persisted manifests keyed by manifest version.
    pub known_manifest_crc_by_version: HashMap<u64, u32>,

    /// Observable manifest-switch trace events from replay.
    pub manifest_switch_traces: Vec<ManifestSwitchRecoveryTrace>,
}

impl ReplayContext {
    pub fn new() -> Self {
        Self {
            last_replayed_lsn: None,
            applied_count: 0,
            skipped_count: 0,
            error_records: Vec::new(),
            active_manifest: None,
            known_manifest_crc_by_version: HashMap::new(),
            manifest_switch_traces: Vec::new(),
        }
    }

    pub fn record_result(&mut self, result: ReplayResult) {
        self.last_replayed_lsn = Some(result.lsn);

        match result.outcome {
            ReplayOutcome::Applied => self.applied_count += 1,
            ReplayOutcome::Skipped => self.skipped_count += 1,
            ReplayOutcome::NotYetImplemented | ReplayOutcome::Deprecated => {
                self.error_records.push(result);
            }
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.error_records.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSwitchRecoveryTrace {
    ManifestSwitchApplied {
        lsn: Lsn,
        manifest_version: u64,
        snapshot_id: u64,
        base_checkpoint_lsn: Lsn,
        required_wal_start_lsn: Lsn,
    },
    ManifestSwitchValidationFailed {
        lsn: Lsn,
        manifest_version: u64,
        reason: &'static str,
    },
}

impl Default for ReplayContext {
    fn default() -> Self {
        Self::new()
    }
}
