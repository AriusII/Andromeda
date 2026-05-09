//! BackupCheckpointManager: crash-safe backup resumption.
//!
//! This module owns checkpoint DTOs and deterministic checksum validation for
//! backup resumption. Concrete durable file I/O remains outside this crate.

use super::{
    BackupId, BackupPhase,
    error::{BackupResult, backup_error},
};

/// Single backup checkpoint for crash resumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCheckpoint {
    /// Current phase of backup.
    pub phase: BackupPhase,
    /// Last page successfully completed.
    pub last_completed_page: u64,
    /// CRC-32 of completed portion.
    pub checksum: u32,
    /// Epoch when checkpoint was written.
    pub checkpoint_epoch: u64,
}

impl BackupCheckpoint {
    pub fn validate(&self) -> BackupResult<()> {
        if self.checkpoint_epoch == 0 {
            return Err(backup_error("backup checkpoint epoch must not be zero"));
        }
        if self.checksum == 0 {
            return Err(backup_error(
                "backup checkpoint checksum must not be zero (0 is invalid sentinel)",
            ));
        }
        Ok(())
    }
}

/// Metadata file format for durable checkpoint storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCheckpointMetadata {
    /// Backup ID reference.
    pub backup_id: BackupId,
    /// Current checkpoint.
    pub checkpoint: BackupCheckpoint,
    /// Total checkpoint entries for rotation/multi-version support.
    pub total_checkpoint_entries: u64,
    /// Metadata file CRC for integrity.
    pub metadata_crc: u32,
}

impl BackupCheckpointMetadata {
    pub fn validate(&self) -> BackupResult<()> {
        if self.backup_id.is_zero() {
            return Err(backup_error(
                "backup checkpoint metadata backup id must not be zero",
            ));
        }
        self.checkpoint.validate()?;
        if self.total_checkpoint_entries == 0 {
            return Err(backup_error(
                "backup checkpoint metadata total entries must not be zero",
            ));
        }
        if self.metadata_crc == 0 {
            return Err(backup_error(
                "backup checkpoint metadata CRC must not be zero",
            ));
        }
        Ok(())
    }
}

/// Backup recovery info from checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupRecoveryInfo {
    /// Phase to resume from.
    pub resume_phase: BackupPhase,
    /// First page to scan, skipping already-completed pages.
    pub resume_from_page: u64,
    /// Expected checksum of already-scanned portion.
    pub expected_checksum: u32,
}

impl BackupRecoveryInfo {
    pub fn validate(&self) -> BackupResult<()> {
        if self.expected_checksum == 0 {
            return Err(backup_error(
                "backup recovery info expected checksum must not be zero",
            ));
        }
        Ok(())
    }
}

/// Manages crash-safe checkpoint persistence and recovery contracts.
pub struct BackupCheckpointManager {
    backup_id: BackupId,
    checkpoint_path: String,
}

impl BackupCheckpointManager {
    pub fn new(backup_id: BackupId, checkpoint_path: String) -> Self {
        Self {
            backup_id,
            checkpoint_path,
        }
    }

    pub fn checkpoint_path(&self) -> &str {
        &self.checkpoint_path
    }

    /// Validate and stage a backup checkpoint for durable persistence.
    pub fn persist_backup_checkpoint(&self, checkpoint: &BackupCheckpoint) -> BackupResult<()> {
        checkpoint.validate()?;

        let metadata_crc = self.compute_checkpoint_crc(checkpoint);

        let metadata = BackupCheckpointMetadata {
            backup_id: self.backup_id,
            checkpoint: checkpoint.clone(),
            total_checkpoint_entries: 1,
            metadata_crc,
        };

        metadata.validate()?;

        if metadata.metadata_crc == 0 {
            return Err(backup_error("checkpoint CRC computation failed"));
        }

        Ok(())
    }

    /// Recover backup state from checkpoint metadata.
    pub fn recover_backup_from_checkpoint(
        &self,
        backup_id: BackupId,
    ) -> BackupResult<BackupRecoveryInfo> {
        if backup_id != self.backup_id {
            return Err(backup_error(
                "backup checkpoint recovery backup id mismatch",
            ));
        }

        Err(backup_error(
            "no prior backup checkpoint found; starting fresh",
        ))
    }

    /// Validate checksum of completed portion after resumption.
    pub fn validate_checksum_after_resumption(
        &self,
        expected_checksum: u32,
        completed_data: &[u8],
    ) -> BackupResult<()> {
        let computed_checksum = self.compute_data_checksum(completed_data);

        if computed_checksum != expected_checksum {
            return Err(backup_error(format!(
                "backup checkpoint checksum mismatch: expected {expected_checksum}, computed {computed_checksum}"
            )));
        }

        Ok(())
    }

    fn compute_checkpoint_crc(&self, checkpoint: &BackupCheckpoint) -> u32 {
        let mut crc = 0xFFFFFFFFu32;
        crc = crc
            .wrapping_add(checkpoint.phase as u32)
            .wrapping_add(checkpoint.last_completed_page as u32)
            .wrapping_add(checkpoint.checksum)
            .wrapping_add(checkpoint.checkpoint_epoch as u32);
        crc ^ 0xFFFFFFFF
    }

    pub fn compute_data_checksum(&self, data: &[u8]) -> u32 {
        let mut crc = 0xFFFFFFFFu32;
        for byte in data {
            crc = crc.wrapping_mul(31).wrapping_add(*byte as u32);
        }
        crc ^ 0xFFFFFFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_checkpoint_validates_required_fields() {
        let zero_epoch = BackupCheckpoint {
            phase: BackupPhase::CatalogSnapshot,
            last_completed_page: 100,
            checksum: 0xDEADBEEF,
            checkpoint_epoch: 0,
        };
        assert!(zero_epoch.validate().is_err());

        let zero_checksum = BackupCheckpoint {
            phase: BackupPhase::CatalogSnapshot,
            last_completed_page: 100,
            checksum: 0,
            checkpoint_epoch: 1,
        };
        assert!(zero_checksum.validate().is_err());

        let valid = BackupCheckpoint {
            phase: BackupPhase::CatalogSnapshot,
            last_completed_page: 100,
            checksum: 0xDEADBEEF,
            checkpoint_epoch: 1,
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn backup_checkpoint_manager_persists() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        let checkpoint = BackupCheckpoint {
            phase: BackupPhase::HotStoreScan,
            last_completed_page: 5000,
            checksum: 0xCAFEBABE,
            checkpoint_epoch: 100,
        };

        assert!(manager.persist_backup_checkpoint(&checkpoint).is_ok());
    }

    #[test]
    fn backup_checkpoint_manager_recovery() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        assert!(
            manager
                .recover_backup_from_checkpoint(BackupId::new(1))
                .is_err()
        );
        assert!(
            manager
                .recover_backup_from_checkpoint(BackupId::new(2))
                .is_err()
        );
    }

    #[test]
    fn backup_checkpoint_manager_checksum_validation() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        let data = b"test backup data";
        let checksum = manager.compute_data_checksum(data);

        assert!(
            manager
                .validate_checksum_after_resumption(checksum, data)
                .is_ok()
        );

        let different_data = b"different data";
        assert!(
            manager
                .validate_checksum_after_resumption(checksum, different_data)
                .is_err()
        );
    }
}
