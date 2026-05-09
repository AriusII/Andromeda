//! Backup and restore audit event contracts.

use std::time::SystemTime;

use andromeda_observability::TraceId;

/// Backup operation identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BackupId(u64);

impl BackupId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Restore recovery stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryStage {
    SafeStart,
    ForensicStart,
}

impl RecoveryStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SafeStart => "safe_start",
            Self::ForensicStart => "forensic_start",
        }
    }
}

/// Restore completion status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreCompletion {
    Success { replayed_lsn: u64 },
    Failed { reason: String },
}

impl RestoreCompletion {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

/// Backup audit event types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupAuditEvent {
    /// Backup operation initiated.
    BackupStarted {
        backup_id: BackupId,
        database_id: u64,
    },

    /// Snapshot checkpoint captured and frozen for backup.
    SnapshotCheckpointCaptured {
        backup_id: BackupId,
        checkpoint_lsn: u64,
        snapshot_id: u64,
    },

    /// WAL segment added to backup archive.
    WalSegmentArchived {
        backup_id: BackupId,
        segment_id: u64,
        lsn_range: (u64, u64),
        checksum: u64,
    },

    /// Backup manifest finalized and ready for restore.
    BackupManifestFinalized {
        backup_id: BackupId,
        manifest_crc: u32,
        earliest_pitr: u64,
        latest_pitr: u64,
    },

    /// Backup artifact validation failed.
    BackupValidationFailed { backup_id: BackupId, reason: String },

    /// Restore operation initiated from this backup.
    RestoreStarted {
        backup_id: BackupId,
        pitr_target_lsn: u64,
        stage: RecoveryStage,
    },

    /// Restore operation completed.
    RestoreCompleted {
        backup_id: BackupId,
        final_checkpoint_lsn: u64,
        status: RestoreCompletion,
    },
}

impl BackupAuditEvent {
    /// Stable event type label.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::BackupStarted { .. } => "backup_started",
            Self::SnapshotCheckpointCaptured { .. } => "snapshot_checkpoint_captured",
            Self::WalSegmentArchived { .. } => "wal_segment_archived",
            Self::BackupManifestFinalized { .. } => "backup_manifest_finalized",
            Self::BackupValidationFailed { .. } => "backup_validation_failed",
            Self::RestoreStarted { .. } => "restore_started",
            Self::RestoreCompleted { .. } => "restore_completed",
        }
    }

    /// Backup id carried by every backup audit event.
    pub fn backup_id(&self) -> BackupId {
        match self {
            Self::BackupStarted { backup_id, .. }
            | Self::SnapshotCheckpointCaptured { backup_id, .. }
            | Self::WalSegmentArchived { backup_id, .. }
            | Self::BackupManifestFinalized { backup_id, .. }
            | Self::BackupValidationFailed { backup_id, .. }
            | Self::RestoreStarted { backup_id, .. }
            | Self::RestoreCompleted { backup_id, .. } => *backup_id,
        }
    }
}

/// Audit trace for a single backup event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupAuditTrace {
    pub trace_id: TraceId,
    pub backup_id: BackupId,
    pub event: BackupAuditEvent,
    pub timestamp_ms: u64,
    pub sequence_number: u64,
}

impl BackupAuditTrace {
    pub fn new(
        trace_id: TraceId,
        backup_id: BackupId,
        event: BackupAuditEvent,
        timestamp: SystemTime,
        sequence_number: u64,
    ) -> Self {
        let timestamp_ms = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            trace_id,
            backup_id,
            event,
            timestamp_ms,
            sequence_number,
        }
    }

    /// Validate required identity and ordering evidence.
    pub fn validate(&self) -> bool {
        !self.trace_id.is_zero() && !self.backup_id.is_zero() && self.sequence_number > 0
    }
}
