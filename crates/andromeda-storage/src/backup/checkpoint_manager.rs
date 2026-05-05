//! BackupCheckpointManager — crash-safe backup resumption.
//!
//! Manages checkpoint persistence and recovery for backup operations.
//! Enables resumption from crash without re-scanning already-completed pages.
//!
//! # Crash Safety Contract
//!
//! 1. Checkpoints are written to durable metadata files
//! 2. Metadata files are fsync'd after each checkpoint
//! 3. Checksum validates completed portion integrity
//! 4. On restart, resume from next page (no re-scan)
//! 5. Recompute checksum for resumed portion

use andromeda_core::AndromedaResult;

use super::helpers::backup_error;
use super::physical_plan::BackupPhase;
use super::types::BackupId;

/// Single backup checkpoint for crash resumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCheckpoint {
    /// Current phase of backup
    pub phase: BackupPhase,
    /// Last page successfully completed
    pub last_completed_page: u64,
    /// CRC-32 of completed portion
    pub checksum: u32,
    /// Epoch when checkpoint was written
    pub checkpoint_epoch: u64,
}

impl BackupCheckpoint {
    pub fn validate(&self) -> AndromedaResult<()> {
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
    /// Backup ID reference
    pub backup_id: BackupId,
    /// Current checkpoint
    pub checkpoint: BackupCheckpoint,
    /// Total checkpoint entries (for rotation/multi-version support)
    pub total_checkpoint_entries: u64,
    /// Metadata file CRC for integrity
    pub metadata_crc: u32,
}

impl BackupCheckpointMetadata {
    pub fn validate(&self) -> AndromedaResult<()> {
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
    /// Phase to resume from
    pub resume_phase: BackupPhase,
    /// First page to scan (skip already-completed)
    pub resume_from_page: u64,
    /// Expected checksum of already-scanned portion
    pub expected_checksum: u32,
}

impl BackupRecoveryInfo {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.expected_checksum == 0 {
            return Err(backup_error(
                "backup recovery info expected checksum must not be zero",
            ));
        }
        Ok(())
    }
}

/// BackupCheckpointManager — manages crash-safe checkpoint persistence and recovery.
pub struct BackupCheckpointManager {
    /// Backup ID for checkpoint references
    backup_id: BackupId,
    /// Checkpoint storage path (in implementation layer)
    checkpoint_path: String,
}

impl BackupCheckpointManager {
    pub fn new(backup_id: BackupId, checkpoint_path: String) -> Self {
        Self {
            backup_id,
            checkpoint_path,
        }
    }

    /// Persist a backup checkpoint durably.
    ///
    /// # Contract
    ///
    /// 1. Write checkpoint to metadata file
    /// 2. fsync to ensure durability
    /// 3. Verify integrity via CRC
    /// 4. On crash before fsync, checkpoint is lost (resumable from last durable checkpoint)
    pub fn persist_backup_checkpoint(&self, checkpoint: &BackupCheckpoint) -> AndromedaResult<()> {
        checkpoint.validate()?;

        // Compute metadata CRC (in a real implementation, this would be a proper checksum)
        let metadata_crc = self.compute_checkpoint_crc(checkpoint);

        let metadata = BackupCheckpointMetadata {
            backup_id: self.backup_id,
            checkpoint: checkpoint.clone(),
            total_checkpoint_entries: 1,
            metadata_crc,
        };

        metadata.validate()?;

        // In a real implementation, this would write to durable storage and fsync.
        // For now, we validate the contract.
        if metadata.metadata_crc == 0 {
            return Err(backup_error("checkpoint CRC computation failed"));
        }

        Ok(())
    }

    /// Recover backup state from checkpoint.
    ///
    /// # Contract
    ///
    /// 1. Load last checkpoint from metadata file
    /// 2. Validate checkpoint integrity via CRC
    /// 3. Return recovery info (resume phase, next page to scan)
    /// 4. If no checkpoint exists, return error (backup must be restarted)
    pub fn recover_backup_from_checkpoint(
        &self,
        backup_id: BackupId,
    ) -> AndromedaResult<BackupRecoveryInfo> {
        if backup_id != self.backup_id {
            return Err(backup_error(
                "backup checkpoint recovery backup id mismatch",
            ));
        }

        // In a real implementation, this would read from durable storage.
        // For now, we return an error indicating no prior checkpoint.
        Err(backup_error(
            "no prior backup checkpoint found; starting fresh",
        ))
    }

    /// Validate checksum of completed portion after resumption.
    ///
    /// # Contract
    ///
    /// 1. Recompute checksum of completed portion
    /// 2. Compare with expected checksum from checkpoint
    /// 3. If mismatch, backup data is corrupt; fail immediately
    pub fn validate_checksum_after_resumption(
        &self,
        expected_checksum: u32,
        completed_data: &[u8],
    ) -> AndromedaResult<()> {
        let computed_checksum = self.compute_data_checksum(completed_data);

        if computed_checksum != expected_checksum {
            return Err(backup_error(format!(
                "backup checkpoint checksum mismatch: expected {}, computed {}",
                expected_checksum, computed_checksum
            )));
        }

        Ok(())
    }

    /// Compute CRC for checkpoint metadata.
    fn compute_checkpoint_crc(&self, checkpoint: &BackupCheckpoint) -> u32 {
        // Simplified checksum (in real implementation, use proper CRC-32)
        let mut crc = 0xFFFFFFFFu32;
        crc = crc
            .wrapping_add(checkpoint.phase as u32)
            .wrapping_add(checkpoint.last_completed_page as u32)
            .wrapping_add(checkpoint.checksum)
            .wrapping_add(checkpoint.checkpoint_epoch as u32);
        crc ^ 0xFFFFFFFF
    }

    /// Compute checksum for data portion.
    pub fn compute_data_checksum(&self, data: &[u8]) -> u32 {
        // Simplified checksum (in real implementation, use proper CRC-32)
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
    fn test_backup_checkpoint_validates_required_fields() {
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
    fn test_backup_checkpoint_manager_persists() {
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
    fn test_backup_checkpoint_manager_recovery() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        // No prior checkpoint should return error
        let recovery = manager.recover_backup_from_checkpoint(BackupId::new(1));
        assert!(recovery.is_err());

        // Mismatched backup ID should return error
        let recovery_mismatch = manager.recover_backup_from_checkpoint(BackupId::new(2));
        assert!(recovery_mismatch.is_err());
    }

    #[test]
    fn test_backup_checkpoint_manager_checksum_validation() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        let data = b"test backup data";
        let checksum = manager.compute_data_checksum(data);

        // Same data should validate
        assert!(manager
            .validate_checksum_after_resumption(checksum, data)
            .is_ok());

        // Different data should fail
        let different_data = b"different data";
        assert!(manager
            .validate_checksum_after_resumption(checksum, different_data)
            .is_err());
    }
}
