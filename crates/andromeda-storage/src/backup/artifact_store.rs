//! Minimal file-backed physical backup artifact store.
//!
//! This is the first runtime boundary beyond contract-only backup planning:
//! callers provide the durable snapshot bytes and WAL segment bytes selected by
//! `BackupExecutionPlan`, and the store persists a bounded artifact directory
//! with a checksummed manifest that restore preflight can verify.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::{Lsn, WAL_FORMAT_VERSION, WalSegmentDescriptor};

use super::{
    artifacts::{
        BackupArtifactDigest, BackupColdSnapshotArtifact, BackupPhysicalArtifactSet,
        BackupWalSegmentArtifact,
    },
    execution_plan::BackupExecutionPlan,
    helpers::backup_error,
    plan::BackupManifest,
    types::BackupId,
};

const ARTIFACT_MANIFEST_FILE_MAGIC: &[u8] = b"ANDROMEDA-BACKUP-ARTIFACT-V1\n";
const ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 1;
const MANIFEST_FILE_HEADER_LEN: usize = ARTIFACT_MANIFEST_FILE_MAGIC.len() + 2 + 8 + 32;
const MANIFEST_FILE_NAME: &str = "backup.manifest";
const SNAPSHOT_FILE_NAME: &str = "snapshot.bin";
const WAL_DIRECTORY_NAME: &str = "wal";

/// WAL archive evidence stored in the durable artifact manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupWalArchiveEvidence {
    pub start_lsn: Lsn,
    pub end_lsn: Lsn,
    pub segment_count: usize,
    pub total_bytes: u64,
}

impl BackupWalArchiveEvidence {
    pub fn validate_against(
        &self,
        manifest: &BackupManifest,
        wal_segments: &[BackupWalSegmentArtifact],
    ) -> AndromedaResult<()> {
        if self.start_lsn != manifest.wal_archive.start {
            return Err(backup_error(
                "backup WAL archive evidence start LSN must match manifest",
            ));
        }
        if self.end_lsn != manifest.wal_archive.end_inclusive {
            return Err(backup_error(
                "backup WAL archive evidence end LSN must match manifest",
            ));
        }
        if self.segment_count != wal_segments.len() {
            return Err(backup_error(
                "backup WAL archive evidence segment count must match artifacts",
            ));
        }

        let mut computed_bytes = 0_u64;
        for segment in wal_segments {
            computed_bytes = computed_bytes
                .checked_add(segment.artifact.byte_len)
                .ok_or_else(|| backup_error("backup WAL archive evidence byte total overflow"))?;
        }
        if self.total_bytes != computed_bytes {
            return Err(backup_error(
                "backup WAL archive evidence bytes must match WAL artifacts",
            ));
        }
        Ok(())
    }
}

/// Decoded durable manifest plus its on-disk paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupArtifactManifestRecord {
    pub manifest: BackupManifest,
    pub source_checkpoint_lsn: Lsn,
    pub wal_archive_evidence: BackupWalArchiveEvidence,
    pub artifact_set: BackupPhysicalArtifactSet,
    pub manifest_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub wal_segment_paths: Vec<PathBuf>,
}

impl BackupArtifactManifestRecord {
    pub fn validate_metadata(&self) -> AndromedaResult<()> {
        self.manifest.validate()?;
        if self.source_checkpoint_lsn.is_zero() {
            return Err(backup_error(
                "backup artifact source checkpoint LSN must not be zero",
            ));
        }
        if self.source_checkpoint_lsn != self.manifest.snapshot.base_checkpoint_lsn {
            return Err(backup_error(
                "backup artifact source checkpoint LSN must match manifest snapshot",
            ));
        }
        if self.wal_segment_paths.len() != self.artifact_set.wal_segments.len() {
            return Err(backup_error(
                "backup artifact WAL path count must match manifest segments",
            ));
        }
        self.artifact_set.validate_against(&self.manifest)?;
        self.wal_archive_evidence
            .validate_against(&self.manifest, &self.artifact_set.wal_segments)
    }

    pub fn wal_segment_descriptors(&self) -> AndromedaResult<Vec<WalSegmentDescriptor>> {
        let mut descriptors = Vec::with_capacity(self.artifact_set.wal_segments.len());
        for segment in &self.artifact_set.wal_segments {
            let record_count = usize::try_from(segment.record_count)
                .map_err(|_| backup_error("backup WAL segment record count exceeds usize"))?;
            descriptors.push(WalSegmentDescriptor {
                format_version: segment.wal_format_version,
                segment_id: segment.segment_id,
                first_lsn: segment.first_lsn,
                last_lsn: segment.last_lsn,
                base_previous_lsn: segment.base_previous_lsn,
                record_count,
            });
        }
        Ok(descriptors)
    }
}

/// Result of writing a physical backup plan to a durable artifact directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupArtifactWriteReport {
    pub backup_id: BackupId,
    pub artifact_root: PathBuf,
    pub manifest_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub wal_segment_paths: Vec<PathBuf>,
    pub source_checkpoint_lsn: Lsn,
    pub artifact_set: BackupPhysicalArtifactSet,
    pub wal_archive_evidence: BackupWalArchiveEvidence,
}

/// File-backed artifact store rooted at a directory controlled by the caller.
#[derive(Debug, Clone)]
pub struct FileBackedBackupArtifactStore {
    root: PathBuf,
}

impl FileBackedBackupArtifactStore {
    pub fn open(root: impl Into<PathBuf>) -> AndromedaResult<Self> {
        let store = Self { root: root.into() };
        fs::create_dir_all(&store.root)
            .map_err(|err| io_error("create backup artifact root", err))?;
        Ok(store)
    }

    pub fn open_existing(root: impl Into<PathBuf>) -> AndromedaResult<Self> {
        let root = root.into();
        if !root.exists() {
            return Err(backup_error("backup artifact root does not exist"));
        }
        if !root.is_dir() {
            return Err(backup_error("backup artifact root must be a directory"));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn backup_dir(&self, backup_id: BackupId) -> PathBuf {
        self.root.join(backup_dir_name(backup_id))
    }

    pub fn write_execution_plan_artifact(
        &self,
        plan: &BackupExecutionPlan,
        snapshot_bytes: &[u8],
        wal_segment_bytes: &[&[u8]],
    ) -> AndromedaResult<BackupArtifactWriteReport> {
        plan.validate()?;

        if snapshot_bytes.len() as u64 != plan.total_extent_bytes {
            return Err(backup_error(
                "backup snapshot artifact bytes must match execution plan extent bytes",
            ));
        }
        if wal_segment_bytes.len() != plan.wal_segment_copy_plan.len() {
            return Err(backup_error(
                "backup WAL artifact count must match execution plan segment count",
            ));
        }

        let backup_dir = self.backup_dir(plan.manifest.backup_id);
        if backup_dir.exists() {
            return Err(backup_error(
                "backup artifact directory already exists; refusing overwrite",
            ));
        }

        let wal_dir = backup_dir.join(WAL_DIRECTORY_NAME);
        fs::create_dir_all(&wal_dir)
            .map_err(|err| io_error("create backup artifact WAL directory", err))?;

        let snapshot_path = backup_dir.join(SNAPSHOT_FILE_NAME);
        write_file_atomically(&snapshot_path, snapshot_bytes)?;
        let snapshot_digest = digest_bytes(snapshot_bytes)?;

        let cold_snapshot = BackupColdSnapshotArtifact {
            database_id: plan.manifest.database_id,
            manifest_version: plan.manifest.created_epoch,
            snapshot_id: plan.manifest.snapshot.snapshot_id,
            snapshot_descriptor_hash: plan.manifest.snapshot.snapshot_descriptor_hash,
            manifest_crc: plan.manifest.manifest_crc,
            artifact: snapshot_digest,
        };

        let mut wal_segments = Vec::with_capacity(plan.wal_segment_copy_plan.len());
        let mut wal_segment_paths = Vec::with_capacity(plan.wal_segment_copy_plan.len());
        for (index, (task, bytes)) in plan
            .wal_segment_copy_plan
            .iter()
            .zip(wal_segment_bytes.iter())
            .enumerate()
        {
            if bytes.len() as u64 != task.byte_count {
                return Err(backup_error(
                    "backup WAL artifact bytes must match execution plan WAL bytes",
                ));
            }
            let descriptor = task.segment_descriptor;
            let wal_path = wal_segment_path(&backup_dir, index, descriptor.segment_id);
            write_file_atomically(&wal_path, bytes)?;
            let record_count = u64::try_from(descriptor.record_count)
                .map_err(|_| backup_error("backup WAL segment record count exceeds u64"))?;
            wal_segments.push(BackupWalSegmentArtifact {
                segment_id: descriptor.segment_id,
                first_lsn: descriptor.first_lsn,
                last_lsn: descriptor.last_lsn,
                base_previous_lsn: descriptor.base_previous_lsn,
                record_count,
                wal_format_version: descriptor.format_version,
                artifact: digest_bytes(bytes)?,
            });
            wal_segment_paths.push(wal_path);
        }

        let mut total_wal_bytes = 0_u64;
        for segment in &wal_segments {
            total_wal_bytes = total_wal_bytes
                .checked_add(segment.artifact.byte_len)
                .ok_or_else(|| backup_error("backup artifact WAL byte total overflow"))?;
        }
        let wal_archive_evidence = BackupWalArchiveEvidence {
            start_lsn: plan.manifest.wal_archive.start,
            end_lsn: plan.manifest.wal_archive.end_inclusive,
            segment_count: wal_segments.len(),
            total_bytes: total_wal_bytes,
        };

        let manifest_payload = encode_manifest_payload(
            &plan.manifest,
            plan.manifest.snapshot.base_checkpoint_lsn,
            &wal_archive_evidence,
            &cold_snapshot,
            &wal_segments,
        )?;
        let manifest_bytes = encode_manifest_file(&manifest_payload)?;
        let manifest_path = backup_dir.join(MANIFEST_FILE_NAME);
        write_file_atomically(&manifest_path, &manifest_bytes)?;
        sync_directory_best_effort(&backup_dir);

        let artifact_set = BackupPhysicalArtifactSet {
            backup_manifest: digest_bytes(&manifest_bytes)?,
            cold_snapshot,
            wal_segments,
        };

        let record = BackupArtifactManifestRecord {
            manifest: plan.manifest,
            source_checkpoint_lsn: plan.manifest.snapshot.base_checkpoint_lsn,
            wal_archive_evidence,
            artifact_set: artifact_set.clone(),
            manifest_path: manifest_path.clone(),
            snapshot_path: snapshot_path.clone(),
            wal_segment_paths: wal_segment_paths.clone(),
        };
        record.validate_metadata()?;
        self.validate_artifact_files(&record)?;

        Ok(BackupArtifactWriteReport {
            backup_id: plan.manifest.backup_id,
            artifact_root: backup_dir,
            manifest_path,
            snapshot_path,
            wal_segment_paths,
            source_checkpoint_lsn: plan.manifest.snapshot.base_checkpoint_lsn,
            artifact_set,
            wal_archive_evidence,
        })
    }

    pub fn load_artifact_manifest(
        &self,
        backup_id: BackupId,
    ) -> AndromedaResult<BackupArtifactManifestRecord> {
        if backup_id.is_zero() {
            return Err(backup_error("backup artifact lookup id must not be zero"));
        }

        let backup_dir = self.backup_dir(backup_id);
        let manifest_path = backup_dir.join(MANIFEST_FILE_NAME);
        let manifest_bytes = fs::read(&manifest_path)
            .map_err(|err| io_error("read backup artifact manifest", err))?;
        let manifest_digest = digest_bytes(&manifest_bytes)?;
        let decoded = decode_manifest_file(&manifest_bytes)?;

        if decoded.manifest.backup_id != backup_id {
            return Err(backup_error(
                "backup artifact manifest id must match requested backup id",
            ));
        }

        let snapshot_path = backup_dir.join(SNAPSHOT_FILE_NAME);
        let wal_segment_paths = decoded
            .wal_segments
            .iter()
            .enumerate()
            .map(|(index, segment)| wal_segment_path(&backup_dir, index, segment.segment_id))
            .collect();

        let record = BackupArtifactManifestRecord {
            manifest: decoded.manifest,
            source_checkpoint_lsn: decoded.source_checkpoint_lsn,
            wal_archive_evidence: decoded.wal_archive_evidence,
            artifact_set: BackupPhysicalArtifactSet {
                backup_manifest: manifest_digest,
                cold_snapshot: decoded.cold_snapshot,
                wal_segments: decoded.wal_segments,
            },
            manifest_path,
            snapshot_path,
            wal_segment_paths,
        };
        record.validate_metadata()?;
        Ok(record)
    }

    pub fn validate_artifact_directory(
        &self,
        backup_id: BackupId,
    ) -> AndromedaResult<BackupArtifactManifestRecord> {
        let record = self.load_artifact_manifest(backup_id)?;
        self.validate_artifact_files(&record)?;
        Ok(record)
    }

    fn validate_artifact_files(
        &self,
        record: &BackupArtifactManifestRecord,
    ) -> AndromedaResult<()> {
        let snapshot_bytes =
            fs::read(&record.snapshot_path).map_err(|err| io_error("read backup snapshot", err))?;
        let snapshot_digest = digest_bytes(&snapshot_bytes)?;
        if snapshot_digest != record.artifact_set.cold_snapshot.artifact {
            return Err(backup_error("backup snapshot artifact checksum mismatch"));
        }

        for (path, segment) in record
            .wal_segment_paths
            .iter()
            .zip(record.artifact_set.wal_segments.iter())
        {
            let wal_bytes =
                fs::read(path).map_err(|err| io_error("read backup WAL segment", err))?;
            let wal_digest = digest_bytes(&wal_bytes)?;
            if wal_digest != segment.artifact {
                return Err(backup_error(
                    "backup WAL segment artifact checksum mismatch",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedArtifactManifest {
    manifest: BackupManifest,
    source_checkpoint_lsn: Lsn,
    wal_archive_evidence: BackupWalArchiveEvidence,
    cold_snapshot: BackupColdSnapshotArtifact,
    wal_segments: Vec<BackupWalSegmentArtifact>,
}

fn encode_manifest_file(payload: &[u8]) -> AndromedaResult<Vec<u8>> {
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| backup_error("backup artifact manifest payload length exceeds u64"))?;
    let payload_checksum = Sha256::digest(payload);
    let mut bytes = Vec::with_capacity(MANIFEST_FILE_HEADER_LEN + payload.len());
    bytes.extend_from_slice(ARTIFACT_MANIFEST_FILE_MAGIC);
    bytes.extend_from_slice(&ARTIFACT_MANIFEST_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(&payload_checksum);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn decode_manifest_file(bytes: &[u8]) -> AndromedaResult<DecodedArtifactManifest> {
    if bytes.len() < MANIFEST_FILE_HEADER_LEN {
        return Err(backup_error("backup artifact manifest file is truncated"));
    }
    if &bytes[..ARTIFACT_MANIFEST_FILE_MAGIC.len()] != ARTIFACT_MANIFEST_FILE_MAGIC {
        return Err(backup_error("backup artifact manifest magic mismatch"));
    }

    let mut offset = ARTIFACT_MANIFEST_FILE_MAGIC.len();
    let version = read_u16_at(bytes, offset)?;
    offset += 2;
    if version != ARTIFACT_MANIFEST_FORMAT_VERSION {
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
    decode_manifest_payload(payload)
}

fn encode_manifest_payload(
    manifest: &BackupManifest,
    source_checkpoint_lsn: Lsn,
    evidence: &BackupWalArchiveEvidence,
    cold_snapshot: &BackupColdSnapshotArtifact,
    wal_segments: &[BackupWalSegmentArtifact],
) -> AndromedaResult<Vec<u8>> {
    manifest.validate()?;
    cold_snapshot.validate_against(manifest)?;
    evidence.validate_against(manifest, wal_segments)?;

    let segment_count = u64::try_from(wal_segments.len())
        .map_err(|_| backup_error("backup artifact WAL segment count exceeds u64"))?;
    let mut payload = Vec::with_capacity(256 + wal_segments.len() * 96);
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

    Ok(payload)
}

fn decode_manifest_payload(payload: &[u8]) -> AndromedaResult<DecodedArtifactManifest> {
    let mut cursor = PayloadCursor::new(payload);
    let manifest = BackupManifest {
        backup_id: BackupId::new(cursor.read_u64()?),
        database_id: cursor.read_u64()?,
        created_epoch: cursor.read_u64()?,
        snapshot: super::types::ColdSnapshotBoundary {
            snapshot_id: cursor.read_u64()?,
            snapshot_descriptor_hash: cursor.read_array_32()?,
            base_checkpoint_lsn: cursor.read_lsn()?,
            required_wal_start_lsn: cursor.read_lsn()?,
        },
        wal_archive: super::types::WalArchiveRange::new(cursor.read_lsn()?, cursor.read_lsn()?),
        manifest_crc: cursor.read_u32()?,
    };
    let source_checkpoint_lsn = cursor.read_lsn()?;
    let wal_archive_evidence = BackupWalArchiveEvidence {
        start_lsn: cursor.read_lsn()?,
        end_lsn: cursor.read_lsn()?,
        segment_count: usize::try_from(cursor.read_u64()?)
            .map_err(|_| backup_error("backup artifact evidence segment count exceeds usize"))?,
        total_bytes: cursor.read_u64()?,
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
    cursor.finish()?;

    let decoded = DecodedArtifactManifest {
        manifest,
        source_checkpoint_lsn,
        wal_archive_evidence,
        cold_snapshot,
        wal_segments,
    };
    decoded.manifest.validate()?;
    if decoded.source_checkpoint_lsn != decoded.manifest.snapshot.base_checkpoint_lsn {
        return Err(backup_error(
            "backup artifact source checkpoint LSN must match manifest snapshot",
        ));
    }
    decoded.cold_snapshot.validate_against(&decoded.manifest)?;
    decoded
        .wal_archive_evidence
        .validate_against(&decoded.manifest, &decoded.wal_segments)?;
    BackupPhysicalArtifactSet {
        backup_manifest: BackupArtifactDigest {
            sha256: [1; 32],
            crc64: 1,
            byte_len: 1,
        },
        cold_snapshot: decoded.cold_snapshot,
        wal_segments: decoded.wal_segments.clone(),
    }
    .validate_against(&decoded.manifest)?;

    Ok(decoded)
}

fn write_file_atomically(path: &Path, bytes: &[u8]) -> AndromedaResult<()> {
    if bytes.is_empty() {
        return Err(backup_error("backup artifact file bytes must not be empty"));
    }
    if path.exists() {
        return Err(backup_error("backup artifact file already exists"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| backup_error("backup artifact path must have a parent directory"))?;
    fs::create_dir_all(parent).map_err(|err| io_error("create backup artifact directory", err))?;

    let tmp_path = path.with_extension("tmp");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path)
            .map_err(|err| io_error("remove stale backup artifact temp file", err))?;
    }
    let mut tmp =
        File::create(&tmp_path).map_err(|err| io_error("create backup artifact temp file", err))?;
    tmp.write_all(bytes)
        .map_err(|err| io_error("write backup artifact temp file", err))?;
    tmp.sync_all()
        .map_err(|err| io_error("sync backup artifact temp file", err))?;
    drop(tmp);
    fs::rename(&tmp_path, path).map_err(|err| io_error("rename backup artifact temp file", err))?;
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> AndromedaResult<BackupArtifactDigest> {
    if bytes.is_empty() {
        return Err(backup_error(
            "backup artifact digest bytes must not be empty",
        ));
    }
    let sha256: [u8; 32] = Sha256::digest(bytes).into();
    let digest = BackupArtifactDigest {
        sha256,
        crc64: crc64(bytes),
        byte_len: u64::try_from(bytes.len())
            .map_err(|_| backup_error("backup artifact byte length exceeds u64"))?,
    };
    digest.validate("backup artifact digest")?;
    Ok(digest)
}

fn crc64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}

fn backup_dir_name(backup_id: BackupId) -> String {
    format!("backup-{:016x}", backup_id.get())
}

fn wal_segment_path(backup_dir: &Path, sequence_index: usize, segment_id: u64) -> PathBuf {
    backup_dir
        .join(WAL_DIRECTORY_NAME)
        .join(format!("segment-{sequence_index:06}-{segment_id:016x}.wal"))
}

fn sync_directory_best_effort(path: &Path) {
    if let Ok(file) = File::open(path) {
        let _ = file.sync_all();
    }
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_lsn(bytes: &mut Vec<u8>, lsn: Lsn) {
    push_u64(bytes, lsn.get());
}

fn push_option_lsn(bytes: &mut Vec<u8>, lsn: Option<Lsn>) {
    match lsn {
        Some(value) => {
            bytes.push(1);
            push_lsn(bytes, value);
        }
        None => {
            bytes.push(0);
            push_u64(bytes, 0);
        }
    }
}

fn push_digest(bytes: &mut Vec<u8>, digest: BackupArtifactDigest) {
    bytes.extend_from_slice(&digest.sha256);
    push_u64(bytes, digest.crc64);
    push_u64(bytes, digest.byte_len);
}

fn read_u16_at(bytes: &[u8], offset: usize) -> AndromedaResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| backup_error("backup artifact manifest offset overflow"))?;
    let array: [u8; 2] = bytes
        .get(offset..end)
        .ok_or_else(|| backup_error("backup artifact manifest u16 field is truncated"))?
        .try_into()
        .map_err(|_| backup_error("backup artifact manifest u16 field is truncated"))?;
    Ok(u16::from_le_bytes(array))
}

fn read_u64_at(bytes: &[u8], offset: usize) -> AndromedaResult<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| backup_error("backup artifact manifest offset overflow"))?;
    let array: [u8; 8] = bytes
        .get(offset..end)
        .ok_or_else(|| backup_error("backup artifact manifest u64 field is truncated"))?
        .try_into()
        .map_err(|_| backup_error("backup artifact manifest u64 field is truncated"))?;
    Ok(u64::from_le_bytes(array))
}

struct PayloadCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn finish(&self) -> AndromedaResult<()> {
        if self.offset != self.bytes.len() {
            return Err(backup_error(
                "backup artifact manifest payload has trailing bytes",
            ));
        }
        Ok(())
    }

    fn read_exact(&mut self, len: usize) -> AndromedaResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| backup_error("backup artifact manifest payload offset overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| backup_error("backup artifact manifest payload is truncated"))?;
        self.offset = end;
        Ok(slice)
    }

    fn read_u64(&mut self) -> AndromedaResult<u64> {
        let array: [u8; 8] = self
            .read_exact(8)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest u64 field is truncated"))?;
        Ok(u64::from_le_bytes(array))
    }

    fn read_u32(&mut self) -> AndromedaResult<u32> {
        let array: [u8; 4] = self
            .read_exact(4)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest u32 field is truncated"))?;
        Ok(u32::from_le_bytes(array))
    }

    fn read_u16(&mut self) -> AndromedaResult<u16> {
        let array: [u8; 2] = self
            .read_exact(2)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest u16 field is truncated"))?;
        Ok(u16::from_le_bytes(array))
    }

    fn read_u8(&mut self) -> AndromedaResult<u8> {
        Ok(*self
            .read_exact(1)?
            .first()
            .ok_or_else(|| backup_error("backup artifact manifest u8 field is truncated"))?)
    }

    fn read_lsn(&mut self) -> AndromedaResult<Lsn> {
        Ok(Lsn::new(self.read_u64()?))
    }

    fn read_option_lsn(&mut self) -> AndromedaResult<Option<Lsn>> {
        let tag = self.read_u8()?;
        let value = Lsn::new(self.read_u64()?);
        match tag {
            0 => {
                if !value.is_zero() {
                    return Err(backup_error(
                        "backup artifact manifest absent LSN must encode zero value",
                    ));
                }
                Ok(None)
            }
            1 => {
                if value.is_zero() {
                    return Err(backup_error(
                        "backup artifact manifest present LSN must not be zero",
                    ));
                }
                Ok(Some(value))
            }
            _ => Err(backup_error(
                "backup artifact manifest optional LSN tag is invalid",
            )),
        }
    }

    fn read_array_32(&mut self) -> AndromedaResult<[u8; 32]> {
        self.read_exact(32)?
            .try_into()
            .map_err(|_| backup_error("backup artifact manifest 32-byte field is truncated"))
    }

    fn read_digest(&mut self) -> AndromedaResult<BackupArtifactDigest> {
        let digest = BackupArtifactDigest {
            sha256: self.read_array_32()?,
            crc64: self.read_u64()?,
            byte_len: self.read_u64()?,
        };
        digest.validate("backup artifact manifest digest")?;
        Ok(digest)
    }
}

fn io_error(action: &str, err: std::io::Error) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, format!("{action}: {err}"))
}

#[allow(dead_code)]
fn _assert_wal_format_version_is_current(version: u16) -> AndromedaResult<()> {
    if version != WAL_FORMAT_VERSION {
        return Err(backup_error("backup artifact WAL format version mismatch"));
    }
    Ok(())
}
