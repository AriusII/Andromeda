use sha2::{Digest, Sha256};

use andromeda_wal::Lsn;

use super::{
    BackupWalArchiveEvidence,
    digest::compute_wal_archive_digest,
    payload_cursor::{
        PayloadCursor, push_digest, push_lsn, push_option_lsn, push_u16, push_u32, push_u64,
        read_u16_at, read_u64_at,
    },
};
use crate::{
    artifacts::{
        BackupArtifactCompatibilityEvidence, BackupArtifactDigest, BackupColdSnapshotArtifact,
        BackupPhysicalArtifactSet as BackupPhysicalArtifactSetRaw,
        BackupWalSegmentArtifact as BackupWalSegmentArtifactRaw,
    },
    error::{BackupResult, backup_error, map_backup_validation},
    plan::BackupManifest as BackupManifestRaw,
    types::{
        BackupId, ColdSnapshotBoundary as ColdSnapshotBoundaryRaw,
        WalArchiveRange as WalArchiveRangeRaw,
    },
};

type BackupManifest = BackupManifestRaw<Lsn>;
type BackupPhysicalArtifactSet = BackupPhysicalArtifactSetRaw<Lsn>;
type BackupWalSegmentArtifact = BackupWalSegmentArtifactRaw<Lsn>;
type ColdSnapshotBoundary = ColdSnapshotBoundaryRaw<Lsn>;
type WalArchiveRange = WalArchiveRangeRaw<Lsn>;

const ARTIFACT_MANIFEST_FILE_MAGIC: &[u8] = b"ANDROMEDA-BACKUP-ARTIFACT-V1\n";
const ARTIFACT_MANIFEST_FORMAT_VERSION_V1: u16 = 1;
const ARTIFACT_MANIFEST_FORMAT_VERSION_V2: u16 = 2;
pub(super) const ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 3;
const MANIFEST_FILE_HEADER_LEN: usize = ARTIFACT_MANIFEST_FILE_MAGIC.len() + 2 + 8 + 32;
const BACKUP_ARTIFACT_MANIFEST_MAX_WAL_SEGMENTS: usize = 16_384;

// Durable manifest compatibility policy:
// - v1 reads are accepted after reconstructing the aggregate WAL archive digest.
// - v2 reads are accepted with the persisted aggregate WAL archive digest.
// - v3 is the only write format and must persist compatibility evidence:
//   manifest format, physical plan format, storage format, and WAL format.
// Unsupported versions fail closed before any restore preflight can proceed.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedArtifactManifest {
    pub(super) format_version: u16,
    pub(super) manifest: BackupManifest,
    pub(super) source_checkpoint_lsn: Lsn,
    pub(super) wal_archive_evidence: BackupWalArchiveEvidence,
    pub(super) compatibility_evidence: BackupArtifactCompatibilityEvidence,
    pub(super) cold_snapshot: BackupColdSnapshotArtifact,
    pub(super) wal_segments: Vec<BackupWalSegmentArtifact>,
}

pub(super) fn encode_manifest_file(payload: &[u8]) -> BackupResult<Vec<u8>> {
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| backup_error("backup artifact manifest payload length exceeds u64"))?;
    let payload_checksum = Sha256::digest(payload);
    let capacity = MANIFEST_FILE_HEADER_LEN
        .checked_add(payload.len())
        .ok_or_else(|| backup_error("backup artifact manifest file length overflow"))?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(ARTIFACT_MANIFEST_FILE_MAGIC);
    bytes.extend_from_slice(&ARTIFACT_MANIFEST_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(&payload_checksum);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

pub(super) fn decode_manifest_file(bytes: &[u8]) -> BackupResult<DecodedArtifactManifest> {
    if bytes.len() < MANIFEST_FILE_HEADER_LEN {
        return Err(backup_error("backup artifact manifest file is truncated"));
    }
    if &bytes[..ARTIFACT_MANIFEST_FILE_MAGIC.len()] != ARTIFACT_MANIFEST_FILE_MAGIC {
        return Err(backup_error("backup artifact manifest magic mismatch"));
    }

    let mut offset = ARTIFACT_MANIFEST_FILE_MAGIC.len();
    let version = read_u16_at(bytes, offset)?;
    offset += 2;
    if !is_supported_manifest_format_version(version) {
        return Err(backup_error(
            "backup artifact manifest format version is unsupported",
        ));
    }

    let payload_len = usize::try_from(read_u64_at(bytes, offset)?)
        .map_err(|_| backup_error("backup artifact manifest payload length exceeds usize"))?;
    offset += 8;
    let checksum_end = offset + 32;
    let expected_checksum = &bytes[offset..checksum_end];
    offset = checksum_end;

    let expected_len = MANIFEST_FILE_HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| backup_error("backup artifact manifest file length overflow"))?;
    if bytes.len() != expected_len {
        return Err(backup_error(
            "backup artifact manifest file length mismatch",
        ));
    }

    let payload = &bytes[offset..];
    let computed: [u8; 32] = Sha256::digest(payload).into();
    if computed.as_slice() != expected_checksum {
        return Err(backup_error(
            "backup artifact manifest payload checksum mismatch",
        ));
    }
    decode_manifest_payload(version, payload)
}

pub(super) fn encode_manifest_payload(
    manifest: &BackupManifest,
    source_checkpoint_lsn: Lsn,
    evidence: &BackupWalArchiveEvidence,
    compatibility_evidence: BackupArtifactCompatibilityEvidence,
    cold_snapshot: &BackupColdSnapshotArtifact,
    wal_segments: &[BackupWalSegmentArtifact],
) -> BackupResult<Vec<u8>> {
    map_backup_validation(manifest.validate())?;
    map_backup_validation(cold_snapshot.validate_against(manifest))?;
    evidence.validate_against(manifest, wal_segments)?;
    map_backup_validation(compatibility_evidence.validate())?;
    if compatibility_evidence.manifest_format_version != ARTIFACT_MANIFEST_FORMAT_VERSION
        || !compatibility_evidence.recorded_in_manifest
    {
        return Err(backup_error(
            "backup artifact current manifest must record compatibility evidence",
        ));
    }

    validate_manifest_wal_segment_count(wal_segments.len(), "backup artifact WAL segment count")?;
    let segment_count = u64::try_from(wal_segments.len())
        .map_err(|_| backup_error("backup artifact WAL segment count exceeds u64"))?;
    let mut payload = Vec::with_capacity(manifest_payload_capacity(wal_segments.len(), true)?);
    push_u64(&mut payload, manifest.backup_id.get());
    push_u64(&mut payload, manifest.database_id);
    push_u64(&mut payload, manifest.created_epoch);
    push_u64(&mut payload, manifest.snapshot.snapshot_id);
    payload.extend_from_slice(&manifest.snapshot.snapshot_descriptor_hash);
    push_lsn(&mut payload, manifest.snapshot.base_checkpoint_lsn);
    push_lsn(&mut payload, manifest.snapshot.required_wal_start_lsn);
    push_lsn(&mut payload, manifest.wal_archive.start);
    push_lsn(&mut payload, manifest.wal_archive.end_inclusive);
    push_u32(&mut payload, manifest.manifest_crc);
    push_lsn(&mut payload, source_checkpoint_lsn);
    push_lsn(&mut payload, evidence.start_lsn);
    push_lsn(&mut payload, evidence.end_lsn);
    push_u64(&mut payload, segment_count);
    push_u64(&mut payload, evidence.total_bytes);
    payload.extend_from_slice(&evidence.archive_digest_sha256);

    push_u64(&mut payload, cold_snapshot.database_id);
    push_u64(&mut payload, cold_snapshot.manifest_version);
    push_u64(&mut payload, cold_snapshot.snapshot_id);
    payload.extend_from_slice(&cold_snapshot.snapshot_descriptor_hash);
    push_u32(&mut payload, cold_snapshot.manifest_crc);
    push_digest(&mut payload, cold_snapshot.artifact);

    push_u64(&mut payload, segment_count);
    for segment in wal_segments {
        push_u64(&mut payload, segment.segment_id);
        push_lsn(&mut payload, segment.first_lsn);
        push_lsn(&mut payload, segment.last_lsn);
        push_option_lsn(&mut payload, segment.base_previous_lsn);
        push_u64(&mut payload, segment.record_count);
        push_u16(&mut payload, segment.wal_format_version);
        push_digest(&mut payload, segment.artifact);
    }

    // v3 compatibility trailer. It is intentionally placed after the segment
    // list so v2 fixtures can be produced by truncating this trailer while
    // keeping all earlier offsets stable for corruption tests.
    push_u16(&mut payload, compatibility_evidence.manifest_format_version);
    push_u16(&mut payload, compatibility_evidence.physical_plan_version);
    push_u16(&mut payload, compatibility_evidence.storage_format_version);
    push_u16(&mut payload, compatibility_evidence.wal_format_version);

    Ok(payload)
}

fn decode_manifest_payload(
    format_version: u16,
    payload: &[u8],
) -> BackupResult<DecodedArtifactManifest> {
    let mut cursor = PayloadCursor::new(payload);
    let manifest = BackupManifest {
        backup_id: BackupId::new(cursor.read_u64()?),
        database_id: cursor.read_u64()?,
        created_epoch: cursor.read_u64()?,
        snapshot: ColdSnapshotBoundary {
            snapshot_id: cursor.read_u64()?,
            snapshot_descriptor_hash: cursor.read_array_32()?,
            base_checkpoint_lsn: cursor.read_lsn()?,
            required_wal_start_lsn: cursor.read_lsn()?,
        },
        wal_archive: WalArchiveRange::new(cursor.read_lsn()?, cursor.read_lsn()?),
        manifest_crc: cursor.read_u32()?,
    };
    let source_checkpoint_lsn = cursor.read_lsn()?;
    let mut wal_archive_evidence = BackupWalArchiveEvidence {
        start_lsn: cursor.read_lsn()?,
        end_lsn: cursor.read_lsn()?,
        segment_count: {
            let count = usize::try_from(cursor.read_u64()?).map_err(|_| {
                backup_error("backup artifact evidence segment count exceeds usize")
            })?;
            validate_manifest_wal_segment_count(count, "backup artifact evidence segment count")?;
            count
        },
        total_bytes: cursor.read_u64()?,
        archive_digest_sha256: match format_version {
            ARTIFACT_MANIFEST_FORMAT_VERSION => cursor.read_array_32()?,
            ARTIFACT_MANIFEST_FORMAT_VERSION_V2 => cursor.read_array_32()?,
            ARTIFACT_MANIFEST_FORMAT_VERSION_V1 => [0; 32],
            _ => {
                return Err(backup_error(
                    "backup artifact manifest format version is unsupported",
                ));
            },
        },
    };

    let cold_snapshot = BackupColdSnapshotArtifact {
        database_id: cursor.read_u64()?,
        manifest_version: cursor.read_u64()?,
        snapshot_id: cursor.read_u64()?,
        snapshot_descriptor_hash: cursor.read_array_32()?,
        manifest_crc: cursor.read_u32()?,
        artifact: cursor.read_digest()?,
    };

    let wal_segment_count = usize::try_from(cursor.read_u64()?)
        .map_err(|_| backup_error("backup artifact WAL segment count exceeds usize"))?;
    validate_manifest_wal_segment_count(wal_segment_count, "backup artifact WAL segment count")?;
    if wal_segment_count != wal_archive_evidence.segment_count {
        return Err(backup_error(
            "backup artifact WAL segment count must match archive evidence",
        ));
    }
    let mut wal_segments = Vec::with_capacity(wal_segment_count);
    for _ in 0..wal_segment_count {
        let segment = BackupWalSegmentArtifact {
            segment_id: cursor.read_u64()?,
            first_lsn: cursor.read_lsn()?,
            last_lsn: cursor.read_lsn()?,
            base_previous_lsn: cursor.read_option_lsn()?,
            record_count: cursor.read_u64()?,
            wal_format_version: cursor.read_u16()?,
            artifact: cursor.read_digest()?,
        };
        wal_segments.push(segment);
    }
    let compatibility_evidence = match format_version {
        ARTIFACT_MANIFEST_FORMAT_VERSION => BackupArtifactCompatibilityEvidence {
            manifest_format_version: cursor.read_u16()?,
            physical_plan_version: cursor.read_u16()?,
            storage_format_version: cursor.read_u16()?,
            wal_format_version: cursor.read_u16()?,
            recorded_in_manifest: true,
        },
        ARTIFACT_MANIFEST_FORMAT_VERSION_V1 | ARTIFACT_MANIFEST_FORMAT_VERSION_V2 => {
            // Older manifests predate explicit compatibility persistence.
            // Restore still gets bounded evidence, but callers can distinguish
            // reconstructed evidence from v3's manifest-recorded evidence.
            BackupArtifactCompatibilityEvidence::reconstructed_legacy(format_version)
        },
        _ => {
            return Err(backup_error(
                "backup artifact manifest format version is unsupported",
            ));
        },
    };
    if format_version == ARTIFACT_MANIFEST_FORMAT_VERSION_V1 {
        wal_archive_evidence.archive_digest_sha256 = compute_wal_archive_digest(&wal_segments);
    }
    cursor.finish()?;

    let decoded = DecodedArtifactManifest {
        format_version,
        manifest,
        source_checkpoint_lsn,
        wal_archive_evidence,
        compatibility_evidence,
        cold_snapshot,
        wal_segments,
    };
    map_backup_validation(decoded.compatibility_evidence.validate())?;
    if decoded.compatibility_evidence.manifest_format_version != decoded.format_version {
        return Err(backup_error(
            "backup artifact compatibility format version must match manifest header",
        ));
    }
    map_backup_validation(decoded.manifest.validate())?;
    if decoded.source_checkpoint_lsn != decoded.manifest.snapshot.base_checkpoint_lsn {
        return Err(backup_error(
            "backup artifact source checkpoint LSN must match manifest snapshot",
        ));
    }
    map_backup_validation(decoded.cold_snapshot.validate_against(&decoded.manifest))?;
    decoded
        .wal_archive_evidence
        .validate_against(&decoded.manifest, &decoded.wal_segments)?;
    map_backup_validation(
        BackupPhysicalArtifactSet {
            backup_manifest: BackupArtifactDigest {
                sha256: [1; 32],
                crc64: 1,
                byte_len: 1,
            },
            cold_snapshot: decoded.cold_snapshot,
            wal_segments: decoded.wal_segments.clone(),
        }
        .validate_against(&decoded.manifest),
    )?;

    Ok(decoded)
}

fn validate_manifest_wal_segment_count(count: usize, label: &str) -> BackupResult<()> {
    if count > BACKUP_ARTIFACT_MANIFEST_MAX_WAL_SEGMENTS {
        return Err(backup_error(format!(
            "{label} exceeds bounded manifest segment limit"
        )));
    }
    Ok(())
}

fn manifest_payload_capacity(
    segment_count: usize,
    has_compatibility_trailer: bool,
) -> BackupResult<usize> {
    let trailer_len = if has_compatibility_trailer { 8 } else { 0 };
    288usize
        .checked_add(
            segment_count
                .checked_mul(91)
                .ok_or_else(|| backup_error("backup artifact manifest segment length overflow"))?,
        )
        .and_then(|len| len.checked_add(trailer_len))
        .ok_or_else(|| backup_error("backup artifact manifest payload length overflow"))
}

pub(super) const fn is_supported_manifest_format_version(version: u16) -> bool {
    matches!(
        version,
        ARTIFACT_MANIFEST_FORMAT_VERSION_V1
            | ARTIFACT_MANIFEST_FORMAT_VERSION_V2
            | ARTIFACT_MANIFEST_FORMAT_VERSION
    )
}
