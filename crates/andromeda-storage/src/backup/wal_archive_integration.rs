//! WAL Archive Integration for Backup Finalization.
//!
//! Integrates WAL shipping (F1/F4) with backup physical plan to finalize
//! a complete, restorable backup.
//!
//! # Contract
//!
//! - WAL archive must be contiguous and include all WAL from catalog snapshot to end
//! - PITR window is [catalog_snapshot_lsn, shipped_wal_lsn]
//! - BackupManifest must be finalized before use in restore

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::physical_plan::BackupPhysicalPlan;
use super::plan::BackupManifest;
use super::types::{BackupId, WalArchiveRange};
use crate::Lsn;

/// Typed rejection reason for WAL archive and PITR window validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalArchiveRejection {
    ValidationFailed,
    ArchiveStartLsnZero,
    ArchiveEndLsnZero,
    ArchiveEndBeforeStart,
    SegmentCountZero,
    FinalizedEventBackupIdZero,
    FinalizedEventEpochZero,
    PitrWindowLsnZero,
    PitrWindowEndBeforeStart,
    CatalogSnapshotLsnZero,
    ShippedStartLsnZero,
    ShippedEndLsnZero,
    ShippedEndBeforeStart,
    ShippedEndBeforeCatalogSnapshot,
    PitrTargetLsnZero,
    PitrTargetBeforeWindow { target_lsn: Lsn, earliest_lsn: Lsn },
    PitrTargetAfterWindow { target_lsn: Lsn, latest_lsn: Lsn },
}

impl WalArchiveRejection {
    fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.to_string())
    }
}

impl std::fmt::Display for WalArchiveRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationFailed => f.write_str("WAL archive validation failed"),
            Self::ArchiveStartLsnZero => f.write_str("WAL archive start LSN must not be zero"),
            Self::ArchiveEndLsnZero => f.write_str("WAL archive end LSN must not be zero"),
            Self::ArchiveEndBeforeStart => f.write_str("WAL archive end must not precede start"),
            Self::SegmentCountZero => f.write_str("WAL archive segment count must not be zero"),
            Self::FinalizedEventBackupIdZero => {
                f.write_str("backup manifest finalized event backup id must not be zero")
            }
            Self::FinalizedEventEpochZero => {
                f.write_str("backup manifest finalized event finalized epoch must not be zero")
            }
            Self::PitrWindowLsnZero => f.write_str("PITR window LSNs must not be zero"),
            Self::PitrWindowEndBeforeStart => f.write_str("PITR window end must not precede start"),
            Self::CatalogSnapshotLsnZero => {
                f.write_str("WAL archive validation requires non-zero catalog snapshot LSN")
            }
            Self::ShippedStartLsnZero => {
                f.write_str("WAL archive shipped start LSN must not be zero")
            }
            Self::ShippedEndLsnZero => f.write_str("WAL archive shipped end LSN must not be zero"),
            Self::ShippedEndBeforeStart => {
                f.write_str("WAL archive shipped end must not precede shipped start")
            }
            Self::ShippedEndBeforeCatalogSnapshot => {
                f.write_str("WAL archive shipped end must cover catalog snapshot LSN")
            }
            Self::PitrTargetLsnZero => f.write_str("PITR target LSN must not be zero"),
            Self::PitrTargetBeforeWindow {
                target_lsn,
                earliest_lsn,
            } => write!(
                f,
                "PITR target LSN {} precedes earliest restorable LSN {}",
                target_lsn.get(),
                earliest_lsn.get()
            ),
            Self::PitrTargetAfterWindow {
                target_lsn,
                latest_lsn,
            } => write!(
                f,
                "PITR target LSN {} exceeds latest restorable LSN {}",
                target_lsn.get(),
                latest_lsn.get()
            ),
        }
    }
}

impl std::error::Error for WalArchiveRejection {}

/// WAL archive validation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalArchiveValidationResult {
    /// True if WAL archive is valid and covers the snapshot
    pub is_valid: bool,
    /// Start LSN of the archive
    pub archive_start_lsn: Lsn,
    /// End LSN of the archive
    pub archive_end_lsn: Lsn,
    /// Number of WAL segments in archive
    pub segment_count: u64,
}

impl WalArchiveValidationResult {
    pub fn validate(&self) -> AndromedaResult<()> {
        if !self.is_valid {
            return Err(WalArchiveRejection::ValidationFailed.into_error());
        }
        if self.archive_start_lsn.is_zero() {
            return Err(WalArchiveRejection::ArchiveStartLsnZero.into_error());
        }
        if self.archive_end_lsn.is_zero() {
            return Err(WalArchiveRejection::ArchiveEndLsnZero.into_error());
        }
        if self.archive_end_lsn < self.archive_start_lsn {
            return Err(WalArchiveRejection::ArchiveEndBeforeStart.into_error());
        }
        if self.segment_count == 0 {
            return Err(WalArchiveRejection::SegmentCountZero.into_error());
        }
        Ok(())
    }
}

/// Manifest event emitted when backup is finalized.
///
/// This corresponds to F7 (audit traces) — indicates that BackupManifest
/// has been created and is ready for restore operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupManifestFinalizedEvent {
    /// Backup ID
    pub backup_id: BackupId,
    /// Finalization epoch
    pub finalized_epoch: u64,
    /// PITR window start (catalog snapshot LSN)
    pub pitr_start_lsn: Lsn,
    /// PITR window end (shipped WAL end LSN)
    pub pitr_end_lsn: Lsn,
}

impl BackupManifestFinalizedEvent {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.backup_id.is_zero() {
            return Err(WalArchiveRejection::FinalizedEventBackupIdZero.into_error());
        }
        if self.finalized_epoch == 0 {
            return Err(WalArchiveRejection::FinalizedEventEpochZero.into_error());
        }
        if self.pitr_start_lsn.is_zero() || self.pitr_end_lsn.is_zero() {
            return Err(WalArchiveRejection::PitrWindowLsnZero.into_error());
        }
        if self.pitr_end_lsn < self.pitr_start_lsn {
            return Err(WalArchiveRejection::PitrWindowEndBeforeStart.into_error());
        }
        Ok(())
    }
}

/// WAL Archive Integration for Backup Finalization.
pub struct WalArchiveIntegration;

impl WalArchiveIntegration {
    /// Validate WAL archive coverage of backup snapshot.
    ///
    /// # Contract
    ///
    /// - WAL archive must start at or before catalog_snapshot_lsn
    /// - WAL archive must end at shipped_wal_lsn
    /// - shipped_wal_lsn >= catalog_snapshot_lsn (WAL must cover snapshot)
    pub fn validate_wal_archive(
        catalog_snapshot_lsn: Lsn,
        shipped_wal_start_lsn: Lsn,
        shipped_wal_end_lsn: Lsn,
        segment_count: u64,
    ) -> AndromedaResult<WalArchiveValidationResult> {
        if catalog_snapshot_lsn.is_zero() {
            return Err(WalArchiveRejection::CatalogSnapshotLsnZero.into_error());
        }

        if shipped_wal_start_lsn.is_zero() {
            return Err(WalArchiveRejection::ShippedStartLsnZero.into_error());
        }

        if shipped_wal_end_lsn.is_zero() {
            return Err(WalArchiveRejection::ShippedEndLsnZero.into_error());
        }

        if shipped_wal_end_lsn < shipped_wal_start_lsn {
            return Err(WalArchiveRejection::ShippedEndBeforeStart.into_error());
        }

        // Key invariant: WAL must cover catalog snapshot
        if shipped_wal_end_lsn < catalog_snapshot_lsn {
            return Err(WalArchiveRejection::ShippedEndBeforeCatalogSnapshot.into_error());
        }

        if segment_count == 0 {
            return Err(WalArchiveRejection::SegmentCountZero.into_error());
        }

        Ok(WalArchiveValidationResult {
            is_valid: true,
            archive_start_lsn: shipped_wal_start_lsn,
            archive_end_lsn: shipped_wal_end_lsn,
            segment_count,
        })
    }

    /// Finalize backup with WAL archive, creating BackupManifest.
    ///
    /// # Contract
    ///
    /// 1. Validate BackupPhysicalPlan
    /// 2. Validate WAL archive covers catalog snapshot
    /// 3. Create BackupManifest with PITR window
    /// 4. Emit BackupManifestFinalizedEvent
    /// 5. Return finalized BackupManifest for restore operations
    pub fn finalize_backup_with_wal_archive(
        plan: &BackupPhysicalPlan,
        manifest_template: &BackupManifest,
        shipped_wal_lsn: Lsn,
        wal_segment_count: u64,
    ) -> AndromedaResult<(BackupManifest, BackupManifestFinalizedEvent)> {
        // Validate physical plan
        plan.validate()?;

        // Validate manifest template
        manifest_template.validate()?;

        // Validate WAL archive covers the snapshot
        let validation = Self::validate_wal_archive(
            plan.catalog_snapshot_lsn,
            manifest_template.wal_archive.start,
            shipped_wal_lsn,
            wal_segment_count,
        )?;

        validation.validate()?;

        // Create finalized manifest with correct WAL range
        let finalized_manifest = BackupManifest {
            backup_id: plan.backup_id,
            database_id: manifest_template.database_id,
            created_epoch: manifest_template.created_epoch,
            snapshot: manifest_template.snapshot,
            wal_archive: WalArchiveRange::new(manifest_template.wal_archive.start, shipped_wal_lsn),
            manifest_crc: manifest_template.manifest_crc,
        };

        finalized_manifest.validate()?;

        // Create audit event
        let event = BackupManifestFinalizedEvent {
            backup_id: plan.backup_id,
            finalized_epoch: plan.created_epoch + 1,
            pitr_start_lsn: finalized_manifest.earliest_pitr_target(),
            pitr_end_lsn: finalized_manifest.latest_pitr_target(),
        };

        event.validate()?;

        Ok((finalized_manifest, event))
    }

    /// Compute PITR window for a finalized backup.
    ///
    /// Returns [earliest_restorable_lsn, latest_restorable_lsn].
    pub fn compute_pitr_window(manifest: &BackupManifest) -> AndromedaResult<(Lsn, Lsn)> {
        manifest.validate()?;

        let earliest = manifest.earliest_pitr_target();
        let latest = manifest.latest_pitr_target();

        if earliest.is_zero() || latest.is_zero() {
            return Err(WalArchiveRejection::PitrWindowLsnZero.into_error());
        }

        if latest < earliest {
            return Err(WalArchiveRejection::PitrWindowEndBeforeStart.into_error());
        }

        Ok((earliest, latest))
    }

    /// Validate that a target LSN falls within PITR window.
    pub fn validate_pitr_target(manifest: &BackupManifest, target_lsn: Lsn) -> AndromedaResult<()> {
        manifest.validate()?;

        if target_lsn.is_zero() {
            return Err(WalArchiveRejection::PitrTargetLsnZero.into_error());
        }

        let (earliest, latest) = Self::compute_pitr_window(manifest)?;

        if target_lsn < earliest {
            return Err(WalArchiveRejection::PitrTargetBeforeWindow {
                target_lsn,
                earliest_lsn: earliest,
            }
            .into_error());
        }

        if target_lsn > latest {
            return Err(WalArchiveRejection::PitrTargetAfterWindow {
                target_lsn,
                latest_lsn: latest,
            }
            .into_error());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{BackupManifest, types::ColdSnapshotBoundary};

    #[test]
    fn test_wal_archive_validation() -> AndromedaResult<()> {
        let catalog_lsn = Lsn::new(1000);
        let wal_start = Lsn::new(900);
        let wal_end = Lsn::new(2000);

        let val = WalArchiveIntegration::validate_wal_archive(catalog_lsn, wal_start, wal_end, 5)?;
        assert!(val.is_valid);
        assert_eq!(val.segment_count, 5);
        Ok(())
    }

    #[test]
    fn test_wal_archive_validation_insufficient_coverage() {
        let catalog_lsn = Lsn::new(2000);
        let wal_start = Lsn::new(900);
        let wal_end = Lsn::new(1000); // Does NOT cover catalog snapshot

        let result =
            WalArchiveIntegration::validate_wal_archive(catalog_lsn, wal_start, wal_end, 5);

        assert!(result.is_err());
    }

    #[test]
    fn test_pitr_window_computation() -> AndromedaResult<()> {
        let snapshot = ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [0xAB; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1000),
        };

        let manifest = BackupManifest {
            backup_id: crate::backup::BackupId::new(1),
            database_id: 42,
            created_epoch: 100,
            snapshot,
            wal_archive: WalArchiveRange::new(Lsn::new(1000), Lsn::new(2000)),
            manifest_crc: 0xDEADBEEF,
        };

        let (earliest, latest) = WalArchiveIntegration::compute_pitr_window(&manifest)?;

        assert_eq!(earliest, Lsn::new(1000));
        assert_eq!(latest, Lsn::new(2000));
        Ok(())
    }

    #[test]
    fn test_pitr_target_validation() {
        let snapshot = ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [0xCD; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1000),
        };

        let manifest = BackupManifest {
            backup_id: crate::backup::BackupId::new(1),
            database_id: 42,
            created_epoch: 100,
            snapshot,
            wal_archive: WalArchiveRange::new(Lsn::new(1000), Lsn::new(2000)),
            manifest_crc: 0xDEADBEEF,
        };

        // Valid target in range
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(1500)).is_ok());

        // Target before earliest
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(500)).is_err());

        // Target after latest
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(3000)).is_err());
    }
}
