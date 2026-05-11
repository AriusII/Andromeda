use andromeda_backup::{BackupArtifactDigest, BackupWalArchiveEvidence};
use andromeda_wal::Lsn;

use super::types::RestoreBackupManifest;

/// Compute deterministic checksum of manifest + WAL archive descriptor.
///
/// Pure function; no I/O or async.
///
/// Used for audit trail verification and replay proof.
/// Result is stable across invocations if manifest and archive are unchanged.
pub fn compute_restore_checksum(manifest: &RestoreBackupManifest) -> u64 {
    let mut hash: u64 = 0;
    hash = mix_restore_checksum(hash, manifest.backup_id.get());
    hash = mix_restore_checksum(hash, manifest.database_id);
    hash = mix_restore_checksum(hash, manifest.created_epoch);
    hash = mix_restore_checksum(hash, manifest.snapshot.snapshot_id);
    hash = mix_bytes(hash, &manifest.snapshot.snapshot_descriptor_hash);
    hash = mix_restore_checksum(hash, manifest.snapshot.base_checkpoint_lsn.get());
    hash = mix_restore_checksum(hash, manifest.snapshot.required_wal_start_lsn.get());
    hash = mix_restore_checksum(hash, manifest.wal_archive.start.get());
    hash = mix_restore_checksum(hash, manifest.wal_archive.end_inclusive.get());
    hash = mix_restore_checksum(hash, u64::from(manifest.manifest_crc));
    if hash == 0 { 1 } else { hash }
}

/// Compute an audit-friendly checksum for restore preflight evidence.
///
/// This binds the selected PITR target to durable artifact evidence: manifest
/// bytes, snapshot bytes, catalog bytes, audit-ledger bytes, and aggregate WAL
/// archive evidence. It is not a replacement for byte-level SHA-256 checks;
/// those are validated before this value is returned.
pub(super) fn compute_restore_preflight_checksum(
    manifest: &RestoreBackupManifest,
    manifest_format_version: u16,
    pitr_target_lsn: Lsn,
    manifest_digest: &BackupArtifactDigest,
    snapshot_digest: &BackupArtifactDigest,
    catalog_digest: &BackupArtifactDigest,
    audit_ledger_digest: &BackupArtifactDigest,
    wal_archive_evidence: &BackupWalArchiveEvidence,
) -> u64 {
    let mut hash = compute_restore_checksum(manifest);
    hash = mix_restore_checksum(hash, u64::from(manifest_format_version));
    hash = mix_restore_checksum(hash, pitr_target_lsn.get());
    hash = mix_artifact_digest(hash, manifest_digest);
    hash = mix_artifact_digest(hash, snapshot_digest);
    hash = mix_artifact_digest(hash, catalog_digest);
    hash = mix_artifact_digest(hash, audit_ledger_digest);
    hash = mix_restore_checksum(hash, wal_archive_evidence.start_lsn.get());
    hash = mix_restore_checksum(hash, wal_archive_evidence.end_lsn.get());
    hash = mix_restore_checksum(hash, wal_archive_evidence.segment_count as u64);
    hash = mix_restore_checksum(hash, wal_archive_evidence.total_bytes);
    hash = mix_bytes(hash, &wal_archive_evidence.archive_digest_sha256);
    if hash == 0 { 1 } else { hash }
}

fn mix_artifact_digest(mut hash: u64, digest: &BackupArtifactDigest) -> u64 {
    hash = mix_bytes(hash, &digest.sha256);
    hash = mix_restore_checksum(hash, digest.crc64);
    mix_restore_checksum(hash, digest.byte_len)
}

fn mix_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash = mix_restore_checksum(hash, u64::from(*byte));
    }
    hash
}

fn mix_restore_checksum(hash: u64, value: u64) -> u64 {
    hash.rotate_left(7)
        .wrapping_mul(0x9E37_79B1_85EB_CA87)
        .wrapping_add(value)
}
