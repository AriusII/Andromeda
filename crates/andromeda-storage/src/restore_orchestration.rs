//! F6 Restore and PITR Execution Plan — deterministic restore orchestration.
//!
//! This module defines the decision model for restoring from a durable backup manifest
//! to a point-in-time (PITR) using WAL replay. All functions are pure (no I/O, no async).
//!
//! # Execution Model
//!
//! 1. **Validate manifest** — signature, WAL bounds, snapshot identity
//! 2. **Select PITR target LSN** — app-driven (not automatic)
//! 3. **Plan replay segments** — identify WAL segments needed to reach PITR LSN
//! 4. **Validate contiguity** — no gaps in LSN chain
//! 5. **Bind audit trace** — trace ID, backup ID, recovery stage
//! 6. **Execute replay** — (deferred to async; plan only)
//!
//! # Design Principles
//!
//! - **No Automatic Decisions**: PITR LSN is always app-selected; no "latest" default
//! - **Forensic Start Support**: ForensicStart mode collects corruption signals without repair
//! - **Durable Validation**: All inputs derive from backup manifest + WAL archive, not hot memory
//! - **Audit Trail Binding**: Each restore is traced with immutable receipt
//! - **Separation of Concerns**: Manifest validation (F5) vs restore planning (F6)

use std::path::{Path, PathBuf};

use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

use crate::{
    Lsn, WalSegmentDescriptor,
    backup::{
        BackupArtifactDigest, BackupId, BackupManifest, BackupWalArchiveEvidence,
        FileBackedBackupArtifactStore,
    },
};

// Helpers

fn restore_error(message: impl Into<String>) -> andromeda_core::AndromedaError {
    andromeda_core::AndromedaError::new(andromeda_core::AndromedaErrorKind::Storage, message)
}

// Types and Orchestration

/// Immutable decision snapshot for a restore attempt.
///
/// Contains all inputs needed to plan replay from backup to a PITR LSN.
/// Orchestration is app-driven (when to execute); this type captures only what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOrchestration {
    /// Durable backup manifest (validated for CRC, version, WAL bounds)
    pub backup_manifest: BackupManifest,

    /// Target LSN for PITR (point-in-time recovery).
    /// Must be exactly the snapshot base checkpoint or within backup's WAL
    /// archive range [start, end_inclusive].
    /// None is NOT allowed; app must be explicit about the target.
    pub pitr_target_lsn: Lsn,

    /// Recovery startup mode (SafeStart or ForensicStart)
    pub recovery_stage: RecoveryStage,

    /// Validation policy for this restore (Full or Minimal verification)
    pub validation_policy: RestoreValidationPolicy,

    /// Immutable audit trace binding this restore to identity + start time
    pub audit: RestoreAuditTrace,
}

impl RestoreOrchestration {
    /// Construct a new restore orchestration snapshot.
    ///
    /// All parameters are captured immutably at construction time.
    pub const fn new(
        backup_manifest: BackupManifest,
        pitr_target_lsn: Lsn,
        recovery_stage: RecoveryStage,
        validation_policy: RestoreValidationPolicy,
        audit: RestoreAuditTrace,
    ) -> Self {
        Self {
            backup_manifest,
            pitr_target_lsn,
            recovery_stage,
            validation_policy,
            audit,
        }
    }

    /// Validate restore orchestration prerequisites.
    ///
    /// Checks:
    /// - Manifest is valid (CRC, identity, WAL bounds)
    /// - PITR target LSN is either exactly the snapshot base checkpoint or
    ///   within backup's WAL archive range
    /// - PITR target LSN does not fall between the snapshot base and required WAL start
    /// - Audit trace binds to the same backup id, target LSN, stage, and manifest checksum
    pub fn validate(&self) -> AndromedaResult<()> {
        validate_restore_prerequisites(&self.backup_manifest, self.pitr_target_lsn)?;
        self.audit.validate()?;
        self.audit.validate_binding(
            &self.backup_manifest,
            self.pitr_target_lsn,
            self.recovery_stage,
        )?;

        if self.recovery_stage == RecoveryStage::ForensicStart
            && self.validation_policy != RestoreValidationPolicy::Full
        {
            return Err(restore_error(
                "ForensicStart restore orchestration requires full validation",
            ));
        }

        Ok(())
    }

    /// Validate restore orchestration against durable artifact preflight evidence.
    ///
    /// This stricter gate is intended for startup paths that have already loaded
    /// a file-backed backup artifact. The audit checksum must bind the complete
    /// preflight evidence, not only the logical backup manifest.
    pub fn validate_with_preflight(
        &self,
        preflight: &RestoreArtifactPreflight,
    ) -> AndromedaResult<()> {
        validate_restore_prerequisites(&self.backup_manifest, self.pitr_target_lsn)?;
        self.audit.validate()?;
        self.audit.validate_preflight_binding(preflight)?;

        if self.backup_manifest.backup_id != preflight.backup_id {
            return Err(restore_error(
                "restore preflight backup ID must match backup manifest",
            ));
        }
        if preflight.validation_policy != self.validation_policy {
            return Err(restore_error(
                "restore preflight validation policy must match orchestration policy",
            ));
        }
        if preflight.source_checkpoint_lsn != self.backup_manifest.snapshot.base_checkpoint_lsn {
            return Err(restore_error(
                "restore preflight source checkpoint LSN must match backup manifest",
            ));
        }

        if self.recovery_stage == RecoveryStage::ForensicStart
            && self.validation_policy != RestoreValidationPolicy::Full
        {
            return Err(restore_error(
                "ForensicStart restore orchestration requires full validation",
            ));
        }

        Ok(())
    }
}

/// Validation policy for restore execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreValidationPolicy {
    /// Verify manifest signature and WAL checksums during replay
    Full,

    /// Skip expensive verification (operator responsibility).
    /// Only checks LSN contiguity, not byte-level integrity.
    Minimal,
}

/// Durable preflight proof for a restore artifact directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreArtifactPreflight {
    pub backup_id: BackupId,
    pub artifact_root: PathBuf,
    pub manifest_format_version: u16,
    pub validation_policy: RestoreValidationPolicy,
    pub pitr_target_lsn: Lsn,
    pub source_checkpoint_lsn: Lsn,
    pub manifest_digest: BackupArtifactDigest,
    pub snapshot_digest: BackupArtifactDigest,
    pub wal_archive_evidence: BackupWalArchiveEvidence,
    pub restore_evidence_checksum: u64,
    pub replay_segment_count: usize,
}

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
) -> AndromedaResult<RestoreArtifactPreflight> {
    let artifact_root = artifact_root.as_ref();
    let store = FileBackedBackupArtifactStore::open_existing(artifact_root)?;
    let record = store.validate_artifact_directory(backup_id)?;

    validate_restore_prerequisites(&record.manifest, pitr_target_lsn)?;
    let wal_descriptors = record.wal_segment_descriptors()?;
    let replay_segments =
        plan_replay_segments(&record.manifest, pitr_target_lsn, &wal_descriptors)?;
    let restore_evidence_checksum = compute_restore_preflight_checksum(
        &record.manifest,
        record.manifest_format_version,
        pitr_target_lsn,
        &record.artifact_set.backup_manifest,
        &record.artifact_set.cold_snapshot.artifact,
        &record.wal_archive_evidence,
    );

    Ok(RestoreArtifactPreflight {
        backup_id,
        artifact_root: store.root().to_path_buf(),
        manifest_format_version: record.manifest_format_version,
        validation_policy,
        pitr_target_lsn,
        source_checkpoint_lsn: record.source_checkpoint_lsn,
        manifest_digest: record.artifact_set.backup_manifest,
        snapshot_digest: record.artifact_set.cold_snapshot.artifact,
        wal_archive_evidence: record.wal_archive_evidence,
        restore_evidence_checksum,
        replay_segment_count: replay_segments.len(),
    })
}

/// Recovery startup stage selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStage {
    /// Standard startup: assume no corruption, replay from checkpoint LSN
    SafeStart,

    /// Forensic startup: detect and report corruption; do not repair in-place
    ForensicStart,
}

impl RecoveryStage {
    /// Whether this stage may open application traffic after recovery gates pass.
    pub const fn application_traffic_allowed(self) -> bool {
        matches!(self, Self::SafeStart)
    }

    /// Whether this stage may mutate durable application truth during startup.
    pub const fn durable_truth_mutation_allowed(self) -> bool {
        matches!(self, Self::SafeStart)
    }

    /// Whether this stage may perform in-place repair.
    pub const fn in_place_repair_allowed(self) -> bool {
        false
    }
}

/// Immutable audit trace capturing restore inputs and final status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreAuditTrace {
    /// Unique trace ID for this restore attempt
    pub trace_id: TraceId,

    /// Identity of the backup being restored
    pub backup_id: BackupId,

    /// Target LSN for this restore
    pub pitr_target_lsn: Lsn,

    /// Recovery stage selected for this restore
    pub stage: RecoveryStage,

    /// Deterministic hash of manifest + WAL archive descriptor
    /// (computed before replay starts)
    pub checksum: u64,

    /// Restore completion status (None until restore finishes)
    pub completion_status: Option<RestoreCompletion>,
}

impl RestoreAuditTrace {
    /// Construct a new audit trace.
    pub const fn new(
        trace_id: TraceId,
        backup_id: BackupId,
        pitr_target_lsn: Lsn,
        stage: RecoveryStage,
        checksum: u64,
    ) -> Self {
        Self {
            trace_id,
            backup_id,
            pitr_target_lsn,
            stage,
            checksum,
            completion_status: None,
        }
    }

    /// Validate audit trace prerequisites.
    ///
    /// Checks:
    /// - Trace ID is not zero (must be unique)
    /// - Backup ID is not zero
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_zero() {
            return Err(restore_error("audit trace ID must not be zero"));
        }

        if self.backup_id.is_zero() {
            return Err(restore_error("backup ID must not be zero"));
        }

        if self.checksum == 0 {
            return Err(restore_error("restore audit checksum must not be zero"));
        }

        Ok(())
    }

    /// Validate that this trace describes the exact restore orchestration being executed.
    pub fn validate_binding(
        &self,
        manifest: &BackupManifest,
        pitr_target_lsn: Lsn,
        stage: RecoveryStage,
    ) -> AndromedaResult<()> {
        if self.backup_id != manifest.backup_id {
            return Err(restore_error(
                "restore audit backup ID must match backup manifest",
            ));
        }
        if self.pitr_target_lsn != pitr_target_lsn {
            return Err(restore_error(
                "restore audit PITR target LSN must match orchestration target",
            ));
        }
        if self.stage != stage {
            return Err(restore_error(
                "restore audit recovery stage must match orchestration stage",
            ));
        }
        if self.checksum != compute_restore_checksum(manifest) {
            return Err(restore_error(
                "restore audit checksum must match backup manifest",
            ));
        }
        Ok(())
    }

    /// Validate that this trace binds to a durable restore artifact preflight.
    pub fn validate_preflight_binding(
        &self,
        preflight: &RestoreArtifactPreflight,
    ) -> AndromedaResult<()> {
        if self.backup_id != preflight.backup_id {
            return Err(restore_error(
                "restore audit backup ID must match preflight backup ID",
            ));
        }
        if self.pitr_target_lsn != preflight.pitr_target_lsn {
            return Err(restore_error(
                "restore audit PITR target LSN must match preflight target",
            ));
        }
        if self.checksum != preflight.restore_evidence_checksum {
            return Err(restore_error(
                "restore audit checksum must match preflight evidence checksum",
            ));
        }
        Ok(())
    }

    /// Bind completion status to this audit trace.
    ///
    /// Returns a new trace with the completion status set.
    pub fn with_completion(mut self, status: RestoreCompletion) -> Self {
        self.completion_status = Some(status);
        self
    }
}

/// Final status of a restore attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreCompletion {
    /// Restore succeeded; recorded the final LSN reached after replay
    Success {
        /// Final LSN reached after WAL replay
        replayed_lsn: Lsn,
        /// Final checkpoint LSN (may be ≤ replayed_lsn if replay incomplete)
        final_checkpoint_lsn: Lsn,
    },

    /// Restore failed; recorded reason for audit trail
    Failed { reason: String },
}

// Pure Functions (RestorePipeline)

/// WAL segment identified for replay during PITR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalSegmentToReplay {
    /// Descriptor of the segment to replay
    pub segment_descriptor: WalSegmentDescriptor,

    /// Position in replay sequence (0 = first segment)
    pub sequence_index: usize,

    /// Whether this segment contains or passes the PITR target LSN
    pub contains_pitr_target: bool,

    /// Last LSN the replay executor may apply from this segment.
    ///
    /// For segments before the target this is the segment's last LSN. For the
    /// segment containing the PITR target this is exactly the requested target
    /// LSN, so replay does not advance to the segment tail by accident.
    pub replay_stop_lsn: Lsn,
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
    // Validate manifest structure
    manifest.validate()?;

    // Validate WAL archive range is valid
    manifest.wal_archive.validate()?;

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

    // Check PITR target is in range
    if !is_snapshot_only_target && !manifest.wal_archive.contains(pitr_lsn) {
        return Err(restore_error(
            "PITR LSN must be within backup WAL archive range",
        ));
    }

    Ok(())
}

/// Plan WAL segment replay sequence to reach PITR target LSN.
///
/// Pure function; no I/O or async.
///
/// Input:
/// - Manifest with cold snapshot + WAL archive range
/// - PITR target LSN (already validated to be in range)
/// - WAL segment descriptors (from archive)
///
/// Output:
/// - Ordered list of segments to replay
/// - Validation: no gaps in LSN chain, contiguity maintained
pub fn plan_replay_segments(
    manifest: &BackupManifest,
    pitr_lsn: Lsn,
    segments: &[WalSegmentDescriptor],
) -> AndromedaResult<Vec<WalSegmentToReplay>> {
    validate_restore_prerequisites(manifest, pitr_lsn)?;

    if pitr_lsn == manifest.snapshot.base_checkpoint_lsn {
        return Ok(Vec::new());
    }

    let first = segments
        .first()
        .ok_or_else(|| restore_error("WAL segment list must not be empty"))?;
    if first.first_lsn != manifest.wal_archive.start {
        return Err(restore_error(
            "first WAL segment must start at backup archive start LSN",
        ));
    }

    let mut replay_plan: Vec<WalSegmentToReplay> = Vec::new();
    let mut expected_previous_lsn: Option<Lsn> = None;

    for (index, seg) in segments.iter().enumerate() {
        // Validate segment structure
        seg.validate()?;

        // Check chain continuity
        if let Some(expected_prev) = expected_previous_lsn {
            if seg.base_previous_lsn != Some(expected_prev) {
                return Err(restore_error(
                    "WAL segment chain has gap: base_previous_lsn mismatch",
                ));
            }
        } else {
            // First segment must have base_previous_lsn = None
            if seg.base_previous_lsn.is_some() {
                return Err(restore_error(
                    "first WAL segment must have base_previous_lsn = None",
                ));
            }
        }

        // Check segment is in range
        if seg.first_lsn < manifest.wal_archive.start
            || seg.last_lsn > manifest.wal_archive.end_inclusive
        {
            return Err(restore_error(
                "WAL segment LSN bounds exceed backup archive range",
            ));
        }

        // Determine if this segment contains the PITR target
        let contains_pitr_target = seg.first_lsn <= pitr_lsn && pitr_lsn <= seg.last_lsn;

        replay_plan.push(WalSegmentToReplay {
            segment_descriptor: *seg,
            sequence_index: index,
            contains_pitr_target,
            replay_stop_lsn: if contains_pitr_target {
                pitr_lsn
            } else {
                seg.last_lsn
            },
        });

        expected_previous_lsn = Some(seg.last_lsn);

        // Stop adding segments after we've covered the PITR target
        if contains_pitr_target {
            break;
        }
    }

    // Verify we found a segment containing the PITR target
    if !replay_plan.iter().any(|seg| seg.contains_pitr_target) {
        return Err(restore_error("no WAL segment contains the PITR target LSN"));
    }

    Ok(replay_plan)
}

/// Compute deterministic checksum of manifest + WAL archive descriptor.
///
/// Pure function; no I/O or async.
///
/// Used for audit trail verification and replay proof.
/// Result is stable across invocations if manifest and archive are unchanged.
pub fn compute_restore_checksum(manifest: &BackupManifest) -> u64 {
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
/// bytes, snapshot bytes, and aggregate WAL archive evidence. It is not a
/// replacement for byte-level SHA-256 checks; those are validated before this
/// value is returned.
pub fn compute_restore_preflight_checksum(
    manifest: &BackupManifest,
    manifest_format_version: u16,
    pitr_target_lsn: Lsn,
    manifest_digest: &BackupArtifactDigest,
    snapshot_digest: &BackupArtifactDigest,
    wal_archive_evidence: &BackupWalArchiveEvidence,
) -> u64 {
    let mut hash = compute_restore_checksum(manifest);
    hash = mix_restore_checksum(hash, u64::from(manifest_format_version));
    hash = mix_restore_checksum(hash, pitr_target_lsn.get());
    hash = mix_artifact_digest(hash, manifest_digest);
    hash = mix_artifact_digest(hash, snapshot_digest);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{ColdSnapshotBoundary, WalArchiveRange};

    fn make_test_manifest() -> BackupManifest {
        BackupManifest {
            backup_id: BackupId::new(1),
            database_id: 1,
            created_epoch: 1,
            snapshot: ColdSnapshotBoundary {
                snapshot_id: 100,
                snapshot_descriptor_hash: [0xAB; 32],
                base_checkpoint_lsn: Lsn::new(1000),
                required_wal_start_lsn: Lsn::new(1001),
            },
            wal_archive: WalArchiveRange::new(Lsn::new(1001), Lsn::new(2000)),
            manifest_crc: 1234,
        }
    }

    #[test]
    fn validate_restore_prerequisites_accepts_valid_manifest() {
        let manifest = make_test_manifest();
        let pitr_lsn = Lsn::new(1500);

        assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
    }

    #[test]
    fn validate_restore_prerequisites_rejects_pitr_before_required_wal_start() {
        let mut manifest = make_test_manifest();
        manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
        manifest.wal_archive = WalArchiveRange::new(Lsn::new(900), Lsn::new(2000));

        let err = validate_restore_prerequisites(&manifest, Lsn::new(1002))
            .expect_err("PITR before required WAL start must fail closed");

        assert!(
            err.message().contains("required WAL start"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_restore_prerequisites_rejects_pitr_below_range() {
        let manifest = make_test_manifest();
        let pitr_lsn = Lsn::new(999); // Below snapshot base checkpoint

        assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
    }

    #[test]
    fn validate_restore_prerequisites_rejects_pitr_above_range() {
        let manifest = make_test_manifest();
        let pitr_lsn = Lsn::new(2001); // Above archive end

        assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
    }

    #[test]
    fn compute_restore_checksum_is_deterministic() {
        let manifest = make_test_manifest();
        let checksum1 = compute_restore_checksum(&manifest);
        let checksum2 = compute_restore_checksum(&manifest);

        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn plan_replay_segments_requires_segments() {
        let manifest = make_test_manifest();
        let pitr_lsn = Lsn::new(1500);

        assert!(plan_replay_segments(&manifest, pitr_lsn, &[]).is_err());
    }

    #[test]
    fn plan_replay_segments_skips_wal_for_snapshot_base_target() {
        let manifest = make_test_manifest();
        let pitr_lsn = Lsn::new(1000);

        let plan = plan_replay_segments(&manifest, pitr_lsn, &[])
            .expect("snapshot base checkpoint target must not need WAL");
        assert!(plan.is_empty());
    }

    #[test]
    fn restore_audit_trace_rejects_zero_trace_id() {
        let trace = RestoreAuditTrace::new(
            TraceId::new(0),
            BackupId::new(1),
            Lsn::new(1500),
            RecoveryStage::SafeStart,
            12345,
        );

        assert!(trace.validate().is_err());
    }

    #[test]
    fn restore_audit_trace_rejects_zero_backup_id() {
        let trace = RestoreAuditTrace::new(
            TraceId::new(1),
            BackupId::new(0),
            Lsn::new(1500),
            RecoveryStage::SafeStart,
            12345,
        );

        assert!(trace.validate().is_err());
    }

    #[test]
    fn restore_orchestration_validates_prerequisites() {
        let manifest = make_test_manifest();
        let audit = RestoreAuditTrace::new(
            TraceId::new(1),
            BackupId::new(1),
            Lsn::new(1500),
            RecoveryStage::SafeStart,
            compute_restore_checksum(&manifest),
        );

        let orch = RestoreOrchestration::new(
            manifest,
            Lsn::new(1500),
            RecoveryStage::SafeStart,
            RestoreValidationPolicy::Full,
            audit,
        );

        assert!(orch.validate().is_ok());
    }

    #[test]
    fn restore_orchestration_rejects_audit_binding_mismatch() {
        let manifest = make_test_manifest();
        let audit = RestoreAuditTrace::new(
            TraceId::new(1),
            BackupId::new(1),
            Lsn::new(1500),
            RecoveryStage::SafeStart,
            compute_restore_checksum(&manifest).wrapping_add(1),
        );

        let orch = RestoreOrchestration::new(
            manifest,
            Lsn::new(1500),
            RecoveryStage::SafeStart,
            RestoreValidationPolicy::Full,
            audit,
        );

        assert!(orch.validate().is_err());
    }

    #[test]
    fn restore_orchestration_rejects_invalid_pitr() {
        let manifest = make_test_manifest();
        let audit = RestoreAuditTrace::new(
            TraceId::new(1),
            BackupId::new(1),
            Lsn::new(3000),
            RecoveryStage::SafeStart,
            compute_restore_checksum(&manifest),
        );

        let orch = RestoreOrchestration::new(
            manifest,
            Lsn::new(3000), // Out of range
            RecoveryStage::SafeStart,
            RestoreValidationPolicy::Full,
            audit,
        );

        assert!(orch.validate().is_err());
    }
}
