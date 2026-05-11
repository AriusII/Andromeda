use std::path::Path;

use andromeda_backup::{BackupId, FileBackedBackupArtifactStore};
use andromeda_wal::Lsn;

use super::{
    checksum::compute_restore_preflight_checksum,
    error::map_backup_validation,
    replay_plan::plan_replay_segments,
    types::{RestoreArtifactPreflight, RestoreValidationPolicy},
    validation::validate_restore_prerequisites,
};

/// Validate a file-backed backup artifact before restore/PITR execution.
///
/// The preflight always validates durable manifest format, manifest checksum,
/// snapshot artifact checksum, WAL segment artifact checksums, WAL chain
/// contiguity, and the requested PITR target. `Minimal` is retained as a future
/// replay-time policy; it does not skip artifact integrity at this boundary.
pub fn validate_restore_artifact_preflight(
    artifact_root: impl AsRef<Path>,
    backup_id: BackupId,
    pitr_target_lsn: Lsn,
    validation_policy: RestoreValidationPolicy,
) -> crate::RestoreResult<RestoreArtifactPreflight> {
    let artifact_root = artifact_root.as_ref();
    let store = map_backup_validation(FileBackedBackupArtifactStore::open_existing(artifact_root))?;
    let record = map_backup_validation(store.validate_artifact_directory(backup_id))?;
    if !record.has_manifest_bound_catalog_and_audit() {
        return Err(crate::restore_error(
            "restore preflight requires current backup artifact manifest with catalog and audit-ledger bindings",
        ));
    }

    validate_restore_prerequisites(&record.manifest, pitr_target_lsn)?;
    let wal_descriptors = map_backup_validation(record.wal_segment_descriptors())?;
    let replay_segments =
        plan_replay_segments(&record.manifest, pitr_target_lsn, &wal_descriptors)?;
    let restore_evidence_checksum = compute_restore_preflight_checksum(
        &record.manifest,
        record.manifest_format_version,
        pitr_target_lsn,
        &record.artifact_set.backup_manifest,
        &record.artifact_set.cold_snapshot.artifact,
        &record.artifact_set.catalog.artifact,
        &record.artifact_set.audit_ledger.artifact,
        &record.wal_archive_evidence,
    );

    Ok(RestoreArtifactPreflight {
        backup_manifest: record.manifest,
        backup_id,
        artifact_root: store.root().to_path_buf(),
        manifest_format_version: record.manifest_format_version,
        validation_policy,
        pitr_target_lsn,
        source_checkpoint_lsn: record.source_checkpoint_lsn,
        manifest_digest: record.artifact_set.backup_manifest,
        snapshot_digest: record.artifact_set.cold_snapshot.artifact,
        catalog_digest: record.artifact_set.catalog.artifact,
        audit_ledger_digest: record.artifact_set.audit_ledger.artifact,
        wal_archive_evidence: record.wal_archive_evidence,
        restore_evidence_checksum,
        replay_segment_count: replay_segments.len(),
    })
}
