//! Minimal file-backed physical backup artifact store.
//!
//! This is the first runtime boundary beyond contract-only backup planning:
//! callers provide the durable snapshot bytes and WAL segment bytes selected by
//! `BackupExecutionPlan`, and the store persists a bounded artifact directory
//! with a checksummed manifest that restore preflight can verify.

mod digest;
mod filesystem;
mod manifest_format;
mod payload_cursor;

use std::{
    fs,
    path::{Path, PathBuf},
};

use andromeda_core::AndromedaResult;

use crate::{Lsn, WalSegmentDescriptor};

use digest::{compute_wal_archive_digest, digest_bytes};
use filesystem::{
    MANIFEST_FILE_NAME, SNAPSHOT_FILE_NAME, WAL_DIRECTORY_NAME, backup_dir_name, io_error,
    read_file_with_expected_len, read_manifest_file, sync_directory_best_effort, wal_segment_path,
    write_file_atomically,
};
use manifest_format::{
    ARTIFACT_MANIFEST_FORMAT_VERSION, decode_manifest_file, encode_manifest_file,
    encode_manifest_payload, is_supported_manifest_format_version,
};

use super::{
    artifacts::{
        BackupArtifactCompatibilityEvidence, BackupColdSnapshotArtifact, BackupPhysicalArtifactSet,
        BackupWalSegmentArtifact,
    },
    execution_plan::BackupExecutionPlan,
    helpers::backup_error,
    plan::BackupManifest,
    types::BackupId,
};

/// WAL archive evidence stored in the durable artifact manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupWalArchiveEvidence {
    pub start_lsn: Lsn,
    pub end_lsn: Lsn,
    pub segment_count: usize,
    pub total_bytes: u64,
    pub archive_digest_sha256: [u8; 32],
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
        if self.segment_count == 0 {
            return Err(backup_error(
                "backup WAL archive evidence segment count must not be zero",
            ));
        }
        if self.archive_digest_sha256 == [0; 32] {
            return Err(backup_error(
                "backup WAL archive evidence digest must not be zero",
            ));
        }

        let computed_bytes = total_wal_artifact_bytes(wal_segments)?;
        if self.total_bytes != computed_bytes {
            return Err(backup_error(
                "backup WAL archive evidence bytes must match WAL artifacts",
            ));
        }
        if self.archive_digest_sha256 != compute_wal_archive_digest(wal_segments) {
            return Err(backup_error(
                "backup WAL archive evidence digest must match WAL artifacts",
            ));
        }
        Ok(())
    }
}

/// Decoded durable manifest plus its on-disk paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupArtifactManifestRecord {
    pub manifest: BackupManifest,
    pub manifest_format_version: u16,
    pub source_checkpoint_lsn: Lsn,
    pub wal_archive_evidence: BackupWalArchiveEvidence,
    pub compatibility_evidence: BackupArtifactCompatibilityEvidence,
    pub artifact_set: BackupPhysicalArtifactSet,
    pub manifest_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub wal_segment_paths: Vec<PathBuf>,
}

impl BackupArtifactManifestRecord {
    pub fn validate_metadata(&self) -> AndromedaResult<()> {
        self.manifest.validate()?;
        if !is_supported_manifest_format_version(self.manifest_format_version) {
            return Err(backup_error(
                "backup artifact manifest format version is unsupported",
            ));
        }
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
        self.compatibility_evidence.validate()?;
        if self.compatibility_evidence.manifest_format_version != self.manifest_format_version {
            return Err(backup_error(
                "backup artifact compatibility format version must match manifest header",
            ));
        }
        if self.manifest_format_version == ARTIFACT_MANIFEST_FORMAT_VERSION
            && !self.compatibility_evidence.recorded_in_manifest
        {
            return Err(backup_error(
                "backup artifact compatibility evidence must be recorded in current manifests",
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
    pub compatibility_evidence: BackupArtifactCompatibilityEvidence,
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

        let wal_archive_evidence = build_wal_archive_evidence(&plan.manifest, &wal_segments)?;

        let manifest_payload = encode_manifest_payload(
            &plan.manifest,
            plan.manifest.snapshot.base_checkpoint_lsn,
            &wal_archive_evidence,
            BackupArtifactCompatibilityEvidence::recorded(ARTIFACT_MANIFEST_FORMAT_VERSION),
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
            manifest_format_version: ARTIFACT_MANIFEST_FORMAT_VERSION,
            source_checkpoint_lsn: plan.manifest.snapshot.base_checkpoint_lsn,
            wal_archive_evidence,
            compatibility_evidence: BackupArtifactCompatibilityEvidence::recorded(
                ARTIFACT_MANIFEST_FORMAT_VERSION,
            ),
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
            compatibility_evidence: BackupArtifactCompatibilityEvidence::recorded(
                ARTIFACT_MANIFEST_FORMAT_VERSION,
            ),
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
        let manifest_bytes = read_manifest_file(&manifest_path)?;
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
            manifest_format_version: decoded.format_version,
            source_checkpoint_lsn: decoded.source_checkpoint_lsn,
            wal_archive_evidence: decoded.wal_archive_evidence,
            compatibility_evidence: decoded.compatibility_evidence,
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
        let snapshot_bytes = read_file_with_expected_len(
            &record.snapshot_path,
            record.artifact_set.cold_snapshot.artifact.byte_len,
            "backup snapshot",
        )?;
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
                read_file_with_expected_len(path, segment.artifact.byte_len, "backup WAL segment")?;
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

fn build_wal_archive_evidence(
    manifest: &BackupManifest,
    wal_segments: &[BackupWalSegmentArtifact],
) -> AndromedaResult<BackupWalArchiveEvidence> {
    Ok(BackupWalArchiveEvidence {
        start_lsn: manifest.wal_archive.start,
        end_lsn: manifest.wal_archive.end_inclusive,
        segment_count: wal_segments.len(),
        total_bytes: total_wal_artifact_bytes(wal_segments)?,
        archive_digest_sha256: compute_wal_archive_digest(wal_segments),
    })
}

fn total_wal_artifact_bytes(wal_segments: &[BackupWalSegmentArtifact]) -> AndromedaResult<u64> {
    wal_segments.iter().try_fold(0_u64, |total_bytes, segment| {
        total_bytes
            .checked_add(segment.artifact.byte_len)
            .ok_or_else(|| backup_error("backup WAL archive evidence byte total overflow"))
    })
}
