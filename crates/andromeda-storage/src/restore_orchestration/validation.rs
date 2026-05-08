use andromeda_backup::BackupResult;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{Lsn, backup::BackupManifest};

use super::{
    error::restore_error,
    types::{
        RecoveryStage, RestoreArtifactPreflight, RestoreOrchestration, RestoreValidationPolicy,
    },
};

pub(super) fn validate_stage_policy(
    recovery_stage: RecoveryStage,
    validation_policy: RestoreValidationPolicy,
) -> AndromedaResult<()> {
    if recovery_stage == RecoveryStage::ForensicStart
        && validation_policy != RestoreValidationPolicy::Full
    {
        return Err(restore_error(
            "ForensicStart restore orchestration requires full validation",
        ));
    }

    Ok(())
}

pub(super) fn validate_preflight_matches_orchestration(
    orchestration: &RestoreOrchestration,
    preflight: &RestoreArtifactPreflight,
) -> AndromedaResult<()> {
    if orchestration.backup_manifest.backup_id != preflight.backup_id {
        return Err(restore_error(
            "restore preflight backup ID must match backup manifest",
        ));
    }
    if preflight.validation_policy != orchestration.validation_policy {
        return Err(restore_error(
            "restore preflight validation policy must match orchestration policy",
        ));
    }
    if preflight.source_checkpoint_lsn != orchestration.backup_manifest.snapshot.base_checkpoint_lsn
    {
        return Err(restore_error(
            "restore preflight source checkpoint LSN must match backup manifest",
        ));
    }

    Ok(())
}

/// Validate restore prerequisites: manifest + PITR target LSN.
///
/// Pure function; no I/O or async.
///
/// Checks:
/// - Manifest identity fields are non-zero
/// - WAL archive range is valid
/// - PITR target LSN is exactly the snapshot base checkpoint, or within
///   [start, end_inclusive]
pub fn validate_restore_prerequisites(
    manifest: &BackupManifest,
    pitr_lsn: Lsn,
) -> AndromedaResult<()> {
    map_backup_validation(manifest.validate())?;
    map_backup_validation(manifest.wal_archive.validate())?;

    if manifest.wal_archive.start > manifest.snapshot.required_wal_start_lsn {
        return Err(restore_error(
            "backup WAL archive must cover snapshot required WAL start LSN",
        ));
    }

    if pitr_lsn < manifest.snapshot.base_checkpoint_lsn {
        return Err(restore_error(
            "PITR LSN must not be before snapshot base checkpoint LSN",
        ));
    }

    let is_snapshot_only_target = pitr_lsn == manifest.snapshot.base_checkpoint_lsn;
    if !is_snapshot_only_target && pitr_lsn < manifest.snapshot.required_wal_start_lsn {
        return Err(restore_error(
            "PITR LSN must not be before snapshot required WAL start LSN",
        ));
    }

    if !is_snapshot_only_target && !manifest.wal_archive.contains(pitr_lsn) {
        return Err(restore_error(
            "PITR LSN must be within backup WAL archive range",
        ));
    }

    Ok(())
}

fn map_backup_validation<T>(result: BackupResult<T>) -> AndromedaResult<T> {
    result.map_err(|error| AndromedaError::new(AndromedaErrorKind::Storage, error.message()))
}
