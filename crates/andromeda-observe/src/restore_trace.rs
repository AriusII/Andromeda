//! Restore and PITR audit trail traces.
//!
//! Captures the immutable inputs and completion status of each restore attempt
//! from backup to point-in-time (PITR).

use crate::TraceId;

/// Unique restore operation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RestoreId(u64);

impl RestoreId {
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

/// Immutable audit trace for a restore operation.
///
/// Binds a restore attempt to a unique trace ID, captures the inputs (backup ID,
/// PITR target LSN, recovery stage), and records the final completion status.
///
/// This type is defined in the observe crate to be referenced from storage events
/// without creating a circular dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTrace {
    /// Unique trace ID for forensic correlation
    pub trace_id: TraceId,

    /// Identity of the backup being restored
    pub backup_id: u64,

    /// Target LSN for point-in-time recovery
    pub pitr_target_lsn: u64,

    /// Recovery stage: "SafeStart" or "ForensicStart"
    pub stage: String,

    /// Deterministic checksum of manifest + WAL archive
    pub checksum: u64,

    /// Completion status: None (in progress), Some(success/failed)
    pub completion_status: Option<RestoreCompletionStatus>,
}

impl RestoreTrace {
    /// Construct a new restore trace.
    pub fn new(
        trace_id: TraceId,
        backup_id: u64,
        pitr_target_lsn: u64,
        stage: &str,
        checksum: u64,
    ) -> Self {
        Self {
            trace_id,
            backup_id,
            pitr_target_lsn,
            stage: stage.to_string(),
            checksum,
            completion_status: None,
        }
    }

    /// Bind completion status to this trace.
    pub fn with_completion(mut self, status: RestoreCompletionStatus) -> Self {
        self.completion_status = Some(status);
        self
    }
}

/// Final status of a restore operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreCompletionStatus {
    /// Restore succeeded
    Success {
        /// Final LSN reached after WAL replay
        replayed_lsn: u64,
        /// Final checkpoint LSN
        final_checkpoint_lsn: u64,
    },

    /// Restore failed with reason
    Failed { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_id_operations_are_stable() {
        let id = RestoreId::new(42);
        assert_eq!(id.get(), 42);
        assert!(!id.is_zero());
        assert!(RestoreId::new(0).is_zero());
    }

    #[test]
    fn restore_trace_captures_inputs() {
        let trace = RestoreTrace::new(TraceId::new(123), 1, 1500, "SafeStart", 12345);
        assert_eq!(trace.trace_id, TraceId::new(123));
        assert_eq!(trace.backup_id, 1);
        assert_eq!(trace.pitr_target_lsn, 1500);
        assert_eq!(trace.checksum, 12345);
        assert!(trace.completion_status.is_none());
    }

    #[test]
    fn restore_trace_binds_completion() {
        let trace = RestoreTrace::new(TraceId::new(123), 1, 1500, "SafeStart", 12345);
        let completed = trace.with_completion(RestoreCompletionStatus::Success {
            replayed_lsn: 1500,
            final_checkpoint_lsn: 1500,
        });

        assert!(completed.completion_status.is_some());
        match completed.completion_status {
            Some(RestoreCompletionStatus::Success {
                replayed_lsn,
                final_checkpoint_lsn,
            }) => {
                assert_eq!(replayed_lsn, 1500);
                assert_eq!(final_checkpoint_lsn, 1500);
            }
            _ => panic!("Expected success"),
        }
    }
}
