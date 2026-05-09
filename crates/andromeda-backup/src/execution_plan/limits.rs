use crate::error::{BackupResult, backup_error};

/// Resource guardrails for a physical backup execution.
///
/// Observed bytes are computed from durable artifact sizes (never from RAM page cache).
/// Backup fails if any limit is exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupResourceLimits {
    /// Maximum bytes for cold snapshot (all extents combined)
    pub max_total_extent_bytes: u64,
    /// Maximum bytes for WAL archive (all segments combined)
    pub max_total_wal_bytes: u64,
    /// Maximum parallel extent copy tasks (design: no implementation constraint here)
    pub max_parallel_extent_tasks: usize,
    /// Maximum WAL segments in archive
    pub max_wal_segment_count: usize,
}

impl BackupResourceLimits {
    pub fn validate(&self) -> BackupResult<()> {
        if self.max_total_extent_bytes == 0
            || self.max_total_wal_bytes == 0
            || self.max_parallel_extent_tasks == 0
            || self.max_wal_segment_count == 0
        {
            return Err(backup_error("backup resource limits must not be zero"));
        }

        Ok(())
    }

    /// Check if an extent plan fits within resource limits.
    pub fn can_accommodate_extents(&self, total_bytes: u64, count: usize) -> BackupResult<()> {
        if total_bytes > self.max_total_extent_bytes {
            return Err(backup_error(
                "extent copy plan exceeds max snapshot bytes limit",
            ));
        }

        if count > self.max_parallel_extent_tasks {
            return Err(backup_error(
                "extent copy plan count exceeds max parallel tasks limit",
            ));
        }

        Ok(())
    }

    /// Check if a WAL segment plan fits within resource limits.
    pub fn can_accommodate_wal(&self, total_bytes: u64, segment_count: usize) -> BackupResult<()> {
        if total_bytes > self.max_total_wal_bytes {
            return Err(backup_error(
                "WAL segment copy plan exceeds max WAL bytes limit",
            ));
        }

        if segment_count > self.max_wal_segment_count {
            return Err(backup_error(
                "WAL segment copy plan exceeds max segment count limit",
            ));
        }

        Ok(())
    }
}
