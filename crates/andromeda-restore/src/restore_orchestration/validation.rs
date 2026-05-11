use andromeda_wal::Lsn;

use super::{
    checksum::compute_restore_preflight_checksum,
    error::{map_backup_validation, restore_error},
    types::{
        RecoveryStage, RestoreArtifactPreflight, RestoreBackupManifest, RestoreOrchestration,
        RestoreValidationPolicy,
    },
};

pub(super) fn validate_stage_policy(
    recovery_stage: RecoveryStage,
    validation_policy: RestoreValidationPolicy,
) -> crate::RestoreResult<()> {
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
) -> crate::RestoreResult<()> {
    if orchestration.backup_manifest.backup_id != preflight.backup_id {
        return Err(restore_error(
            "restore preflight backup ID must match backup manifest",
        ));
    }
    if orchestration.backup_manifest != preflight.backup_manifest {
        return Err(restore_error(
            "restore preflight backup manifest must match orchestration manifest",
        ));
    }
    if preflight.manifest_format_version != 4 {
        return Err(restore_error(
            "restore preflight requires current backup artifact manifest format",
        ));
    }
    if preflight.validation_policy != orchestration.validation_policy {
        return Err(restore_error(
            "restore preflight validation policy must match orchestration policy",
        ));
    }
    if preflight.pitr_target_lsn != orchestration.pitr_target_lsn {
        return Err(restore_error(
            "restore preflight PITR target LSN must match orchestration target",
        ));
    }
    if preflight.source_checkpoint_lsn != orchestration.backup_manifest.snapshot.base_checkpoint_lsn
    {
        return Err(restore_error(
            "restore preflight source checkpoint LSN must match backup manifest",
        ));
    }
    let computed_checksum = compute_restore_preflight_checksum(
        &preflight.backup_manifest,
        preflight.manifest_format_version,
        preflight.pitr_target_lsn,
        &preflight.manifest_digest,
        &preflight.snapshot_digest,
        &preflight.catalog_digest,
        &preflight.audit_ledger_digest,
        &preflight.wal_archive_evidence,
    );
    if preflight.restore_evidence_checksum != computed_checksum {
        return Err(restore_error(
            "restore preflight evidence checksum must match preflight artifact graph",
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
    manifest: &RestoreBackupManifest,
    pitr_lsn: Lsn,
) -> crate::RestoreResult<()> {
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
