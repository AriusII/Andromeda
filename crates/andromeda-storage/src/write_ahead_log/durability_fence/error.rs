use andromeda_core::{AndromedaError, AndromedaErrorKind};

/// Error type for WAL durability fence violations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurabilityFenceError {
    /// Page LSN exceeds the WAL's durable LSN (page cannot be safely flushed).
    PageLsnNotDurable { page_lsn: u64, max_durable_lsn: u64 },
    /// Manifest checkpoint LSN exceeds WAL's checkpoint LSN (manifest cannot switch).
    ManifestCheckpointNotDurable {
        manifest_checkpoint_lsn: u64,
        wal_checkpoint_lsn: u64,
    },
    /// WAL checkpoint evidence itself is ahead of the durable WAL prefix.
    WalCheckpointNotDurable {
        wal_checkpoint_lsn: u64,
        wal_durable_lsn: u64,
    },
    /// Manifest required WAL start LSN exceeds recovery floor (recovery would miss data).
    RecoveryFloorBeforeRequired {
        recovery_floor_lsn: u64,
        required_wal_start_lsn: u64,
    },
    /// LSN ordering violation: one LSN should strictly precede another.
    LsnOrderingViolation {
        earlier_name: String,
        earlier_lsn: u64,
        later_name: String,
        later_lsn: u64,
    },
}

impl DurabilityFenceError {
    fn message(&self) -> String {
        match self {
            Self::PageLsnNotDurable {
                page_lsn,
                max_durable_lsn,
            } => format!(
                "page LSN {} exceeds WAL durable LSN {}; page cannot be safely flushed",
                page_lsn, max_durable_lsn
            ),
            Self::ManifestCheckpointNotDurable {
                manifest_checkpoint_lsn,
                wal_checkpoint_lsn,
            } => format!(
                "manifest checkpoint LSN {} exceeds WAL checkpoint LSN {}; \
                 manifest switch is not safe",
                manifest_checkpoint_lsn, wal_checkpoint_lsn
            ),
            Self::WalCheckpointNotDurable {
                wal_checkpoint_lsn,
                wal_durable_lsn,
            } => format!(
                "WAL checkpoint LSN {} exceeds durable WAL LSN {}; \
                 manifest switch checkpoint evidence is not durable",
                wal_checkpoint_lsn, wal_durable_lsn
            ),
            Self::RecoveryFloorBeforeRequired {
                recovery_floor_lsn,
                required_wal_start_lsn,
            } => format!(
                "recovery floor LSN {} is before manifest required WAL start LSN {}; \
                 recovery would miss required data",
                recovery_floor_lsn, required_wal_start_lsn
            ),
            Self::LsnOrderingViolation {
                earlier_name,
                earlier_lsn,
                later_name,
                later_lsn,
            } => format!(
                "LSN ordering violation: {} ({}) should precede {} ({})",
                earlier_name, earlier_lsn, later_name, later_lsn
            ),
        }
    }

    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.message())
    }
}
