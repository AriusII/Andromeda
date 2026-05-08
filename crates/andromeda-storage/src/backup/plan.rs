//! Compatibility facade for backup manifest DTOs and PITR validation.

use andromeda_core::AndromedaResult;

use crate::Lsn;
use crate::backup::helpers::backup_validation_error;
use crate::backup::helpers::restore_validation_error;

use andromeda_backup::{
    BackupManifest as BackupManifestRaw,
    validate_wal_segment_chain as validate_backup_wal_segment_chain,
};
use andromeda_restore::{
    PitrAuditRecord as RestorePitrAuditRecord, PitrBackupManifest, PitrTarget as RestorePitrTarget,
    PitrValidationAccepted as RestorePitrValidationAccepted,
    validate_pitr_target as validate_restore_pitr_target,
    validate_pitr_target_with_audit as validate_restore_pitr_target_with_audit,
};

use super::{artifacts::BackupWalSegmentArtifact, types::WalArchiveRange as WalArchiveRangeAlias};

pub type BackupManifest = BackupManifestRaw<Lsn>;
pub type PitrTarget = RestorePitrTarget<Lsn>;
pub type PitrValidationAccepted = RestorePitrValidationAccepted<super::types::BackupId, Lsn>;
pub type PitrAuditRecord = RestorePitrAuditRecord<super::types::BackupId, Lsn>;
pub use andromeda_restore::PitrTargetRejection;

struct StorageManifestForRestore<'a>(&'a BackupManifest);

impl<'a> PitrBackupManifest<Lsn, super::types::BackupId> for StorageManifestForRestore<'a> {
    fn backup_manifest_valid(&self) -> bool {
        self.0.validate().is_ok()
    }

    fn backup_id(&self) -> super::types::BackupId {
        self.0.backup_id
    }

    fn database_id(&self) -> u64 {
        self.0.database_id
    }

    fn snapshot_id(&self) -> u64 {
        self.0.snapshot.snapshot_id
    }

    fn base_checkpoint_lsn(&self) -> Lsn {
        self.0.snapshot.base_checkpoint_lsn
    }

    fn required_wal_start_lsn(&self) -> Lsn {
        self.0.snapshot.required_wal_start_lsn
    }

    fn wal_archive_start(&self) -> Lsn {
        self.0.wal_archive.start
    }

    fn wal_archive_end_inclusive(&self) -> Lsn {
        self.0.wal_archive.end_inclusive
    }
}

pub(super) fn validate_wal_segment_chain(
    archive: WalArchiveRangeAlias,
    segments: &[BackupWalSegmentArtifact],
) -> AndromedaResult<()> {
    validate_backup_wal_segment_chain(archive, segments)
        .map_err(|error| backup_validation_error(error.message()))
}

pub fn validate_pitr_target(
    manifest: &BackupManifest,
    target: PitrTarget,
) -> AndromedaResult<PitrValidationAccepted> {
    validate_restore_pitr_target(&StorageManifestForRestore(manifest), target)
        .map_err(|error| restore_validation_error(error.message()))
}

pub fn validate_pitr_target_with_audit(
    manifest: &BackupManifest,
    target: PitrTarget,
) -> (AndromedaResult<PitrValidationAccepted>, PitrAuditRecord) {
    let (result, audit) =
        validate_restore_pitr_target_with_audit(&StorageManifestForRestore(manifest), target);
    (
        result.map_err(|error| restore_validation_error(error.message())),
        audit,
    )
}
