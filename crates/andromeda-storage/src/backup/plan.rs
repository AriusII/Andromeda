//! Compatibility facade for backup manifest DTOs and PITR validation.

use andromeda_core::AndromedaResult;

use crate::Lsn;

use super::{artifacts::BackupWalSegmentArtifact, types::BackupId};

pub type BackupManifest = super::backup_owner::BackupManifest<Lsn>;
pub type PitrTarget = super::restore_owner::PitrTarget<Lsn>;
pub type PitrValidationAccepted = super::restore_owner::PitrValidationAccepted<BackupId, Lsn>;
pub type PitrAuditRecord = super::restore_owner::PitrAuditRecord<BackupId, Lsn>;
pub use super::restore_owner::PitrTargetRejection;

pub(super) fn validate_wal_segment_chain(
    archive: super::types::WalArchiveRange,
    segments: &[BackupWalSegmentArtifact],
) -> AndromedaResult<()> {
    super::backup_owner::validate_wal_segment_chain(archive, segments).map_err(Into::into)
}

pub fn validate_pitr_target(
    manifest: &BackupManifest,
    target: PitrTarget,
) -> AndromedaResult<PitrValidationAccepted> {
    super::restore_owner::validate_pitr_target(manifest, target).map_err(Into::into)
}

pub fn validate_pitr_target_with_audit(
    manifest: &BackupManifest,
    target: PitrTarget,
) -> (AndromedaResult<PitrValidationAccepted>, PitrAuditRecord) {
    let (result, audit) = super::restore_owner::validate_pitr_target_with_audit(manifest, target);
    (result.map_err(Into::into), audit)
}
