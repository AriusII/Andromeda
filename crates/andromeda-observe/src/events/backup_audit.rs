//! Backup and recovery (PITR) operational audit event types.
//!
//! This module defines the audit event taxonomy for backup operations, including:
//! - Backup initiation and completion
//! - Snapshot capture and WAL archival
//! - Manifest finalization and validation
//! - Restore initiation and completion
//!
//! Each event captures a critical decision point or state transition in the backup and
//! recovery lifecycle, enabling forensic analysis and compliance auditing of recovery operations.
//!
//! ## Audit Principles
//!
//! - **Immutability**: Once emitted, an audit event is immutable and append-only.
//! - **Completeness**: Every backup lifecycle transition is traced.
//! - **Traceability**: Backup events carry `backup_id` for correlation with backup artifacts.
//! - **PITR Accuracy**: Earliest and latest recoverable LSN tracked for restore planning.
//! - **No Silent Drops**: All backup audit events are validated before emission.

use std::time::SystemTime;

use crate::TraceId;

/// Unique backup operation identifier.
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

/// Recovery stage for restore operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryStage {
    /// Safe recovery: replayed to a consistent checkpoint.
    SafeStart,
    /// Forensic recovery: replayed to the specified PITR target LSN (may be inconsistent).
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

/// Restore completion status: success or failure with reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreCompletion {
    /// Restore succeeded; final LSN reached.
    Success { replayed_lsn: u64 },
    /// Restore failed; reason describes root cause.
    Failed { reason: String },
}

impl RestoreCompletion {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

/// Backup audit event types. Each event is immutable once created and represents
/// a critical state transition in the backup and recovery lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupAuditEvent {
    /// Backup operation initiated.
    ///
    /// Emitted by: `andromeda-storage::backup::plan` or admin surface
    /// Triggering condition: Backup requested by operator or scheduled policy.
    BackupStarted {
        /// Unique backup identifier assigned at creation time.
        backup_id: BackupId,
        /// Database ID being backed up (for multi-database systems).
        database_id: u64,
    },

    /// Snapshot checkpoint captured and frozen for backup.
    ///
    /// Emitted by: `andromeda-storage::backup::plan`
    /// Triggering condition: Snapshot mechanism creates a consistent point-in-time image.
    SnapshotCheckpointCaptured {
        /// Backup this snapshot belongs to.
        backup_id: BackupId,
        /// LSN of the checkpoint (earliest recoverable point in this backup).
        checkpoint_lsn: u64,
        /// Unique snapshot identifier within the backup.
        snapshot_id: u64,
    },

    /// WAL segment added to backup archive.
    ///
    /// Emitted by: `andromeda-storage::backup::plan`
    /// Triggering condition: WAL segment archived and validated as part of backup.
    WalSegmentArchived {
        /// Backup this segment belongs to.
        backup_id: BackupId,
        /// Unique WAL segment identifier.
        segment_id: u64,
        /// LSN range of archived segment [start, end).
        lsn_range: (u64, u64),
        /// Deterministic CRC32 checksum of segment.
        checksum: u64,
    },

    /// Backup manifest finalized and ready for restore.
    ///
    /// Emitted by: `andromeda-storage::backup::plan`
    /// Triggering condition: Backup completion verification succeeds.
    /// Invariant: Emitted after all WAL segments archived and verified.
    BackupManifestFinalized {
        /// Backup whose manifest is finalized.
        backup_id: BackupId,
        /// CRC32 checksum of the entire manifest.
        manifest_crc: u32,
        /// Earliest LSN recoverable from this backup (from snapshot).
        earliest_pitr: u64,
        /// Latest LSN recoverable from this backup (after WAL replay).
        latest_pitr: u64,
    },

    /// Backup artifact validation failed.
    ///
    /// Emitted by: `andromeda-storage::backup::execution_plan` (on-demand validation)
    /// Triggering condition: Backup verify operation detects corruption or missing artifacts.
    BackupValidationFailed {
        /// Backup that failed validation.
        backup_id: BackupId,
        /// Machine-parseable reason code and message.
        reason: String,
    },

    /// Restore operation initiated from this backup.
    ///
    /// Emitted by: `andromeda-observe::restore_trace` or admin surface
    /// Triggering condition: Operator or recovery system initiates restore from backup.
    RestoreStarted {
        /// Backup being restored from.
        backup_id: BackupId,
        /// Target LSN for point-in-time recovery (may differ from latest_pitr).
        pitr_target_lsn: u64,
        /// Recovery stage (SafeStart or ForensicStart).
        stage: RecoveryStage,
    },

    /// Restore operation completed.
    ///
    /// Emitted by: `andromeda-observe::restore_trace` or recovery orchestrator
    /// Triggering condition: Restore (snapshot load + WAL replay) finishes.
    RestoreCompleted {
        /// Backup restored from.
        backup_id: BackupId,
        /// Final LSN reached after replay.
        final_checkpoint_lsn: u64,
        /// Completion status: Success or reason for failure.
        status: RestoreCompletion,
    },
}

impl BackupAuditEvent {
    /// Human-readable event type for logging and audit reporting.
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

    /// Extract backup ID from any backup audit event (all events are backup-specific).
    pub fn backup_id(&self) -> BackupId {
        match self {
            Self::BackupStarted { backup_id, .. } => *backup_id,
            Self::SnapshotCheckpointCaptured { backup_id, .. } => *backup_id,
            Self::WalSegmentArchived { backup_id, .. } => *backup_id,
            Self::BackupManifestFinalized { backup_id, .. } => *backup_id,
            Self::BackupValidationFailed { backup_id, .. } => *backup_id,
            Self::RestoreStarted { backup_id, .. } => *backup_id,
            Self::RestoreCompleted { backup_id, .. } => *backup_id,
        }
    }
}

/// Immutable audit trace for a single backup event.
///
/// Binds a backup lifecycle event to:
/// - A unique `trace_id` for forensic correlation.
/// - A `backup_id` for correlation with backup artifacts and restore operations.
/// - A `timestamp` for event sequencing.
/// - A `sequence_number` for deterministic ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupAuditTrace {
    /// Unique trace ID for forensic correlation across this backup's lifecycle.
    pub trace_id: TraceId,

    /// Backup operation this event belongs to.
    pub backup_id: BackupId,

    /// The backup audit event itself.
    pub event: BackupAuditEvent,

    /// Timestamp when event was observed (SystemTime in milliseconds since Unix epoch).
    pub timestamp_ms: u64,

    /// Monotonically increasing sequence number for ordering within a single backup context.
    /// Used to establish causal ordering of backup lifecycle events.
    pub sequence_number: u64,
}

impl BackupAuditTrace {
    /// Construct a new backup audit trace.
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

    /// Check that trace has valid evidence for audit acceptance.
    pub fn validate(&self) -> bool {
        !self.trace_id.is_zero() && !self.backup_id.is_zero() && self.sequence_number > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_id_operations_are_stable() {
        let backup_id = BackupId::new(42);
        assert_eq!(backup_id.get(), 42);
        assert!(!backup_id.is_zero());
        assert!(BackupId::new(0).is_zero());
    }

    #[test]
    fn recovery_stage_labels_are_consistent() {
        assert_eq!(RecoveryStage::SafeStart.as_str(), "safe_start");
        assert_eq!(RecoveryStage::ForensicStart.as_str(), "forensic_start");
    }

    #[test]
    fn restore_completion_success_predicate_is_accurate() {
        let success = RestoreCompletion::Success { replayed_lsn: 1500 };
        assert!(success.is_success());

        let failed = RestoreCompletion::Failed {
            reason: "WAL gap detected".to_string(),
        };
        assert!(!failed.is_success());
    }

    #[test]
    fn backup_audit_event_type_labels_are_correct() {
        let event = BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 1,
        };
        assert_eq!(event.event_type(), "backup_started");

        let event = BackupAuditEvent::BackupManifestFinalized {
            backup_id: BackupId::new(1),
            manifest_crc: 12345,
            earliest_pitr: 1000,
            latest_pitr: 2000,
        };
        assert_eq!(event.event_type(), "backup_manifest_finalized");
    }

    #[test]
    fn backup_audit_event_backup_id_extraction_is_consistent() {
        let backup_id = BackupId::new(99);

        let event = BackupAuditEvent::BackupStarted {
            backup_id,
            database_id: 1,
        };
        assert_eq!(event.backup_id(), backup_id);

        let event = BackupAuditEvent::RestoreStarted {
            backup_id,
            pitr_target_lsn: 1500,
            stage: RecoveryStage::SafeStart,
        };
        assert_eq!(event.backup_id(), backup_id);
    }

    #[test]
    fn backup_audit_trace_validates_required_fields() {
        let trace = BackupAuditTrace::new(
            TraceId::new(0), // Invalid: zero trace ID
            BackupId::new(1),
            BackupAuditEvent::BackupStarted {
                backup_id: BackupId::new(1),
                database_id: 1,
            },
            SystemTime::now(),
            1,
        );
        assert!(!trace.validate()); // Should fail due to zero trace ID

        let trace = BackupAuditTrace::new(
            TraceId::new(123),
            BackupId::new(0), // Invalid: zero backup ID
            BackupAuditEvent::BackupStarted {
                backup_id: BackupId::new(1),
                database_id: 1,
            },
            SystemTime::now(),
            1,
        );
        assert!(!trace.validate()); // Should fail due to zero backup ID

        let trace = BackupAuditTrace::new(
            TraceId::new(123),
            BackupId::new(1),
            BackupAuditEvent::BackupStarted {
                backup_id: BackupId::new(1),
                database_id: 1,
            },
            SystemTime::now(),
            0, // Invalid: zero sequence number
        );
        assert!(!trace.validate()); // Should fail due to zero sequence number

        let trace = BackupAuditTrace::new(
            TraceId::new(123),
            BackupId::new(1),
            BackupAuditEvent::BackupStarted {
                backup_id: BackupId::new(1),
                database_id: 1,
            },
            SystemTime::now(),
            1,
        );
        assert!(trace.validate()); // Should pass all checks
    }

    #[test]
    fn backup_audit_trace_captures_timestamp_in_milliseconds() {
        let now = SystemTime::now();
        let trace = BackupAuditTrace::new(
            TraceId::new(123),
            BackupId::new(1),
            BackupAuditEvent::BackupStarted {
                backup_id: BackupId::new(1),
                database_id: 1,
            },
            now,
            1,
        );

        // Verify timestamp is in reasonable range (not zero, recent)
        assert!(trace.timestamp_ms > 0);
        assert!(trace.timestamp_ms < u64::MAX);
    }
}
