use std::path::{Path, PathBuf};

use andromeda_backup::{
    BackupArtifactDigest, BackupId, BackupManifest as BackupManifestRaw, BackupWalArchiveEvidence,
};
use andromeda_observability::TraceId;
use andromeda_wal::Lsn;

use super::{
    checksum::compute_restore_checksum,
    error::RestorePlanError,
    error::restore_error,
    replay_plan::{ReplaySegmentPlanSummaryV0, WalSegmentToReplay},
    validation::{
        validate_preflight_matches_orchestration, validate_restore_prerequisites,
        validate_stage_policy,
    },
};

pub type RestoreBackupManifest = BackupManifestRaw<Lsn>;

/// Immutable decision snapshot for a restore attempt.
///
/// Contains all inputs needed to plan replay from backup to a PITR LSN.
/// Orchestration is app-driven (when to execute); this type captures only what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOrchestration {
    /// Durable backup manifest (validated for CRC, version, WAL bounds)
    pub backup_manifest: RestoreBackupManifest,

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
        backup_manifest: RestoreBackupManifest,
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
}

pub type RestorePlanResult<T> = Result<T, RestorePlanError>;

/// Immutable restore planning receipt for P13 RestorePlan v0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlanV0 {
    backup_id: BackupId,
    manifest_checksum: u64,
    pitr_target_lsn: Lsn,
    replay_segments: Vec<ReplaySegmentPlanSummaryV0>,
    replay_stop_lsn: Lsn,
    preflight_checksum: u64,
    recovery_stage: RecoveryStage,
    validation_policy: RestoreValidationPolicy,
    plan_identity: u64,
}

impl RestorePlanV0 {
    pub const CODEC_VERSION_V0: u16 = 1;
    const CODEC_MAGIC: [u8; 8] = *b"ARPLANV0";
    const CODEC_REPLAY_SEGMENT_LEN: usize = 33;
    const CODEC_TRAILER_LEN: usize = 26;

    pub fn from_orchestration(
        orchestration: &RestoreOrchestration,
        replay_segments: &[WalSegmentToReplay],
        preflight_checksum: u64,
    ) -> RestorePlanResult<Self> {
        if orchestration.pitr_target_lsn.is_zero() {
            return Err(RestorePlanError::ExplicitPitrTargetRequired);
        }
        if preflight_checksum == 0 {
            return Err(RestorePlanError::PreflightChecksumMissing);
        }
        validate_orchestration_for_plan(orchestration, preflight_checksum)?;

        let manifest_checksum = compute_restore_checksum(&orchestration.backup_manifest);
        if manifest_checksum == 0 {
            return Err(RestorePlanError::ManifestChecksumMissing);
        }

        let replay_segments: Vec<ReplaySegmentPlanSummaryV0> = replay_segments
            .iter()
            .map(ReplaySegmentPlanSummaryV0::from)
            .collect();
        validate_replay_summary(
            &orchestration.backup_manifest,
            orchestration.pitr_target_lsn,
            &replay_segments,
        )?;

        let replay_stop_lsn = replay_segments
            .last()
            .map_or(orchestration.pitr_target_lsn, |segment| {
                segment.replay_stop_lsn
            });
        let mut plan = Self {
            backup_id: orchestration.backup_manifest.backup_id,
            manifest_checksum,
            pitr_target_lsn: orchestration.pitr_target_lsn,
            replay_segments,
            replay_stop_lsn,
            preflight_checksum,
            recovery_stage: orchestration.recovery_stage,
            validation_policy: orchestration.validation_policy,
            plan_identity: 0,
        };
        plan.plan_identity = compute_restore_plan_identity(&plan);

        Ok(plan)
    }

    pub const fn backup_id(&self) -> BackupId {
        self.backup_id
    }

    pub const fn manifest_checksum(&self) -> u64 {
        self.manifest_checksum
    }

    pub const fn pitr_target_lsn(&self) -> Lsn {
        self.pitr_target_lsn
    }

    pub fn replay_segments(&self) -> &[ReplaySegmentPlanSummaryV0] {
        &self.replay_segments
    }

    pub const fn replay_stop_lsn(&self) -> Lsn {
        self.replay_stop_lsn
    }

    pub const fn preflight_checksum(&self) -> u64 {
        self.preflight_checksum
    }

    pub const fn recovery_stage(&self) -> RecoveryStage {
        self.recovery_stage
    }

    pub const fn validation_policy(&self) -> RestoreValidationPolicy {
        self.validation_policy
    }

    pub const fn plan_identity(&self) -> u64 {
        self.plan_identity
    }

    pub fn encode_binary_v0(&self) -> Vec<u8> {
        let replay_segment_count = self.replay_segments.len() as u64;
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&Self::CODEC_MAGIC);
        encoded.extend_from_slice(&Self::CODEC_VERSION_V0.to_le_bytes());
        encoded.extend_from_slice(&self.backup_id.get().to_le_bytes());
        encoded.extend_from_slice(&self.manifest_checksum.to_le_bytes());
        encoded.extend_from_slice(&self.pitr_target_lsn.get().to_le_bytes());
        encoded.extend_from_slice(&replay_segment_count.to_le_bytes());
        for segment in &self.replay_segments {
            encoded.extend_from_slice(&(segment.sequence_index as u64).to_le_bytes());
            encoded.extend_from_slice(&segment.first_lsn.get().to_le_bytes());
            encoded.extend_from_slice(&segment.last_lsn.get().to_le_bytes());
            encoded.extend_from_slice(&segment.replay_stop_lsn.get().to_le_bytes());
            encoded.push(u8::from(segment.contains_pitr_target));
        }
        encoded.extend_from_slice(&self.replay_stop_lsn.get().to_le_bytes());
        encoded.extend_from_slice(&self.preflight_checksum.to_le_bytes());
        encoded.push(encode_recovery_stage(self.recovery_stage));
        encoded.push(encode_validation_policy(self.validation_policy));
        encoded.extend_from_slice(&self.plan_identity.to_le_bytes());
        encoded
    }

    pub fn decode_binary_v0(encoded: &[u8]) -> RestorePlanResult<Self> {
        let mut decoder = PlanCodecReader::new(encoded);
        let magic = decoder.read_array::<8>()?;
        if magic != Self::CODEC_MAGIC {
            return Err(RestorePlanError::PlanCodecMagicMismatch);
        }

        let version = decoder.read_u16()?;
        if version != Self::CODEC_VERSION_V0 {
            return Err(RestorePlanError::PlanCodecUnsupportedVersion { version });
        }

        let backup_id = BackupId::new(decoder.read_u64()?);
        if backup_id.is_zero() {
            return Err(RestorePlanError::PlanCodecValueInvalid {
                field: "backup_id",
                message: "must not be zero",
            });
        }

        let manifest_checksum = decoder.read_u64()?;
        if manifest_checksum == 0 {
            return Err(RestorePlanError::ManifestChecksumMissing);
        }

        let pitr_target_lsn = Lsn::new(decoder.read_u64()?);
        if pitr_target_lsn.is_zero() {
            return Err(RestorePlanError::ExplicitPitrTargetRequired);
        }

        let replay_segment_count_u64 = decoder.read_u64()?;
        let replay_segment_count = usize::try_from(replay_segment_count_u64).map_err(|_| {
            RestorePlanError::PlanCodecValueInvalid {
                field: "replay_segment_count",
                message: "does not fit platform usize",
            }
        })?;
        let replay_segment_bytes = replay_segment_count
            .checked_mul(Self::CODEC_REPLAY_SEGMENT_LEN)
            .ok_or(RestorePlanError::PlanCodecValueInvalid {
                field: "replay_segment_count",
                message: "encoded segment bytes overflow",
            })?;
        let expected_remaining = replay_segment_bytes
            .checked_add(Self::CODEC_TRAILER_LEN)
            .ok_or(RestorePlanError::PlanCodecValueInvalid {
                field: "replay_segment_count",
                message: "encoded payload bytes overflow",
            })?;
        if expected_remaining > decoder.remaining_len() {
            return Err(RestorePlanError::PlanCodecTruncated);
        }
        if expected_remaining < decoder.remaining_len() {
            return Err(RestorePlanError::PlanCodecTrailingBytes);
        }

        let mut replay_segments = Vec::with_capacity(replay_segment_count);
        for _ in 0..replay_segment_count {
            let sequence_index = usize::try_from(decoder.read_u64()?).map_err(|_| {
                RestorePlanError::PlanCodecValueInvalid {
                    field: "sequence_index",
                    message: "does not fit platform usize",
                }
            })?;
            let first_lsn = Lsn::new(decoder.read_u64()?);
            let last_lsn = Lsn::new(decoder.read_u64()?);
            let replay_stop_lsn = Lsn::new(decoder.read_u64()?);
            let contains_pitr_target =
                decode_binary_flag(decoder.read_u8()?, "contains_pitr_target")?;
            replay_segments.push(ReplaySegmentPlanSummaryV0 {
                sequence_index,
                first_lsn,
                last_lsn,
                replay_stop_lsn,
                contains_pitr_target,
            });
        }

        let replay_stop_lsn = Lsn::new(decoder.read_u64()?);
        if replay_stop_lsn.is_zero() {
            return Err(RestorePlanError::PlanCodecValueInvalid {
                field: "replay_stop_lsn",
                message: "must not be zero",
            });
        }

        let preflight_checksum = decoder.read_u64()?;
        if preflight_checksum == 0 {
            return Err(RestorePlanError::PreflightChecksumMissing);
        }

        let recovery_stage = decode_recovery_stage(decoder.read_u8()?)?;
        let validation_policy = decode_validation_policy(decoder.read_u8()?)?;
        let plan_identity = decoder.read_u64()?;

        if !decoder.is_eof() {
            return Err(RestorePlanError::PlanCodecTrailingBytes);
        }

        let plan = Self {
            backup_id,
            manifest_checksum,
            pitr_target_lsn,
            replay_segments,
            replay_stop_lsn,
            preflight_checksum,
            recovery_stage,
            validation_policy,
            plan_identity,
        };
        validate_decoded_replay_summary(&plan.replay_segments, plan.pitr_target_lsn)?;
        if plan.replay_stop_lsn
            != plan
                .replay_segments
                .last()
                .map_or(plan.pitr_target_lsn, |segment| segment.replay_stop_lsn)
        {
            return Err(RestorePlanError::ReplayStopMismatch);
        }

        let expected_identity = compute_restore_plan_identity(&plan);
        if plan.plan_identity == 0 || plan.plan_identity != expected_identity {
            return Err(RestorePlanError::PlanIdentityMismatch);
        }

        Ok(plan)
    }
}

struct PlanCodecReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PlanCodecReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_array<const N: usize>(&mut self) -> RestorePlanResult<[u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(RestorePlanError::PlanCodecTruncated)?;
        if end > self.bytes.len() {
            return Err(RestorePlanError::PlanCodecTruncated);
        }
        let mut value = [0_u8; N];
        value.copy_from_slice(&self.bytes[self.offset..end]);
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> RestorePlanResult<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u16(&mut self) -> RestorePlanResult<u16> {
        Ok(u16::from_le_bytes(self.read_array::<2>()?))
    }

    fn read_u64(&mut self) -> RestorePlanResult<u64> {
        Ok(u64::from_le_bytes(self.read_array::<8>()?))
    }

    fn is_eof(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn remaining_len(&self) -> usize {
        self.bytes.len() - self.offset
    }
}

fn validate_orchestration_for_plan(
    orchestration: &RestoreOrchestration,
    preflight_checksum: u64,
) -> RestorePlanResult<()> {
    validate_restore_prerequisites(
        &orchestration.backup_manifest,
        orchestration.pitr_target_lsn,
    )
    .map_err(|error| RestorePlanError::orchestration_invalid(&error))?;
    orchestration
        .audit
        .validate()
        .map_err(|error| RestorePlanError::orchestration_invalid(&error))?;
    validate_stage_policy(
        orchestration.recovery_stage,
        orchestration.validation_policy,
    )
    .map_err(|error| RestorePlanError::orchestration_invalid(&error))?;

    if orchestration.audit.backup_id != orchestration.backup_manifest.backup_id {
        return Err(RestorePlanError::OrchestrationInvalid {
            message: "restore audit backup ID must match backup manifest".to_string(),
        });
    }
    if orchestration.audit.pitr_target_lsn != orchestration.pitr_target_lsn {
        return Err(RestorePlanError::OrchestrationInvalid {
            message: "restore audit PITR target LSN must match orchestration target".to_string(),
        });
    }
    if orchestration.audit.stage != orchestration.recovery_stage {
        return Err(RestorePlanError::OrchestrationInvalid {
            message: "restore audit recovery stage must match orchestration stage".to_string(),
        });
    }

    if orchestration.audit.checksum != preflight_checksum {
        return Err(RestorePlanError::OrchestrationInvalid {
            message: "restore audit checksum must match durable preflight evidence checksum"
                .to_string(),
        });
    }

    Ok(())
}

/// Completion evidence bound to an immutable `RestorePlanV0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlanCompletionEvidenceV0 {
    pub plan_identity: u64,
    pub backup_id: BackupId,
    pub target_lsn: Lsn,
    pub replay_stop_lsn: Lsn,
    pub completion: RestoreCompletion,
}

impl RestorePlanCompletionEvidenceV0 {
    pub fn bind(
        plan: &RestorePlanV0,
        completion: RestoreCompletion,
        preflight_checksum: u64,
    ) -> RestorePlanResult<Self> {
        if preflight_checksum == 0 || preflight_checksum != plan.preflight_checksum {
            return Err(RestorePlanError::PreflightChecksumMismatch);
        }
        if let RestoreCompletion::Success { replayed_lsn, .. } = completion {
            if replayed_lsn != plan.replay_stop_lsn {
                return Err(RestorePlanError::CompletionReplayMismatch);
            }
        }

        Ok(Self {
            plan_identity: plan.plan_identity,
            backup_id: plan.backup_id,
            target_lsn: plan.pitr_target_lsn,
            replay_stop_lsn: plan.replay_stop_lsn,
            completion,
        })
    }
}

impl RestoreOrchestration {
    /// Validate restore orchestration prerequisites.
    ///
    /// Checks:
    /// - Manifest is valid (CRC, identity, WAL bounds)
    /// - PITR target LSN is either exactly the snapshot base checkpoint or
    ///   within backup's WAL archive range
    /// - PITR target LSN does not fall between the snapshot base and required WAL start
    /// - Audit trace binds to the same backup id, target LSN, stage, and manifest checksum
    pub fn validate(&self) -> crate::RestoreResult<()> {
        validate_restore_prerequisites(&self.backup_manifest, self.pitr_target_lsn)?;
        self.audit.validate()?;
        self.audit.validate_binding(
            &self.backup_manifest,
            self.pitr_target_lsn,
            self.recovery_stage,
        )?;
        validate_stage_policy(self.recovery_stage, self.validation_policy)?;

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
    ) -> crate::RestoreResult<()> {
        validate_restore_prerequisites(&self.backup_manifest, self.pitr_target_lsn)?;
        self.audit.validate()?;
        self.audit.validate_preflight_binding(preflight)?;
        validate_preflight_matches_orchestration(self, preflight)?;
        validate_stage_policy(self.recovery_stage, self.validation_policy)?;

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
    pub(super) backup_manifest: RestoreBackupManifest,
    pub(super) backup_id: BackupId,
    pub(super) artifact_root: PathBuf,
    pub(super) manifest_format_version: u16,
    pub(super) validation_policy: RestoreValidationPolicy,
    pub(super) pitr_target_lsn: Lsn,
    pub(super) source_checkpoint_lsn: Lsn,
    pub(super) manifest_digest: BackupArtifactDigest,
    pub(super) snapshot_digest: BackupArtifactDigest,
    pub(super) catalog_digest: BackupArtifactDigest,
    pub(super) audit_ledger_digest: BackupArtifactDigest,
    pub(super) wal_archive_evidence: BackupWalArchiveEvidence,
    pub(super) restore_evidence_checksum: u64,
    pub(super) replay_segment_count: usize,
}

impl RestoreArtifactPreflight {
    pub const fn backup_manifest(&self) -> &RestoreBackupManifest {
        &self.backup_manifest
    }

    pub const fn backup_id(&self) -> BackupId {
        self.backup_id
    }

    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }

    pub const fn manifest_format_version(&self) -> u16 {
        self.manifest_format_version
    }

    pub const fn validation_policy(&self) -> RestoreValidationPolicy {
        self.validation_policy
    }

    pub const fn pitr_target_lsn(&self) -> Lsn {
        self.pitr_target_lsn
    }

    pub const fn source_checkpoint_lsn(&self) -> Lsn {
        self.source_checkpoint_lsn
    }

    pub const fn manifest_digest(&self) -> BackupArtifactDigest {
        self.manifest_digest
    }

    pub const fn snapshot_digest(&self) -> BackupArtifactDigest {
        self.snapshot_digest
    }

    pub const fn catalog_digest(&self) -> BackupArtifactDigest {
        self.catalog_digest
    }

    pub const fn audit_ledger_digest(&self) -> BackupArtifactDigest {
        self.audit_ledger_digest
    }

    pub const fn wal_archive_evidence(&self) -> BackupWalArchiveEvidence {
        self.wal_archive_evidence
    }

    pub const fn restore_evidence_checksum(&self) -> u64 {
        self.restore_evidence_checksum
    }

    pub const fn replay_segment_count(&self) -> usize {
        self.replay_segment_count
    }
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
    pub fn validate(&self) -> crate::RestoreResult<()> {
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
        manifest: &RestoreBackupManifest,
        pitr_target_lsn: Lsn,
        stage: RecoveryStage,
    ) -> crate::RestoreResult<()> {
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
    ) -> crate::RestoreResult<()> {
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

fn validate_replay_summary(
    manifest: &RestoreBackupManifest,
    pitr_target_lsn: Lsn,
    replay_segments: &[ReplaySegmentPlanSummaryV0],
) -> RestorePlanResult<()> {
    let snapshot_only_target = pitr_target_lsn == manifest.snapshot.base_checkpoint_lsn;
    if snapshot_only_target {
        if replay_segments.is_empty() {
            return Ok(());
        }
        return Err(RestorePlanError::UnexpectedReplayForSnapshotTarget);
    }

    if replay_segments.is_empty() {
        return Err(RestorePlanError::MissingReplayCoverage);
    }

    let mut expected_first_lsn = manifest.wal_archive.start;
    let tail_index = replay_segments.len() - 1;

    for (expected_index, segment) in replay_segments.iter().enumerate() {
        if expected_index != segment.sequence_index {
            return Err(RestorePlanError::ReplaySegmentOrderInvalid);
        }
        if segment.first_lsn != expected_first_lsn {
            return Err(RestorePlanError::ReplaySegmentChainInvalid);
        }
        if segment.replay_stop_lsn < segment.first_lsn || segment.replay_stop_lsn > segment.last_lsn
        {
            return Err(RestorePlanError::ReplayStopMismatch);
        }
        if segment.contains_pitr_target != (expected_index == tail_index) {
            return Err(RestorePlanError::MissingReplayCoverage);
        }
        if expected_index == tail_index {
            if segment.replay_stop_lsn != pitr_target_lsn {
                return Err(RestorePlanError::ReplayStopMismatch);
            }
        } else if segment.replay_stop_lsn != segment.last_lsn {
            return Err(RestorePlanError::ReplayStopMismatch);
        }
        expected_first_lsn = segment
            .last_lsn
            .checked_next()
            .ok_or(RestorePlanError::ReplaySegmentChainInvalid)?;
    }

    let tail = replay_segments
        .last()
        .expect("replay segments are guaranteed non-empty");
    if !tail.contains_pitr_target {
        return Err(RestorePlanError::MissingReplayCoverage);
    }
    if tail.replay_stop_lsn != pitr_target_lsn {
        return Err(RestorePlanError::ReplayStopMismatch);
    }

    Ok(())
}

fn compute_restore_plan_identity(plan: &RestorePlanV0) -> u64 {
    let mut hash = 0_u64;
    hash = mix_plan_identity(hash, plan.backup_id.get());
    hash = mix_plan_identity(hash, plan.manifest_checksum);
    hash = mix_plan_identity(hash, plan.pitr_target_lsn.get());
    hash = mix_plan_identity(hash, plan.preflight_checksum);
    hash = mix_plan_identity(hash, stage_discriminant(plan.recovery_stage));
    hash = mix_plan_identity(hash, policy_discriminant(plan.validation_policy));
    hash = mix_plan_identity(hash, plan.replay_segments.len() as u64);
    for segment in &plan.replay_segments {
        hash = mix_plan_identity(hash, segment.sequence_index as u64);
        hash = mix_plan_identity(hash, segment.first_lsn.get());
        hash = mix_plan_identity(hash, segment.last_lsn.get());
        hash = mix_plan_identity(hash, segment.replay_stop_lsn.get());
        hash = mix_plan_identity(hash, u64::from(segment.contains_pitr_target));
    }

    if hash == 0 { 1 } else { hash }
}

const fn encode_recovery_stage(stage: RecoveryStage) -> u8 {
    match stage {
        RecoveryStage::SafeStart => 1,
        RecoveryStage::ForensicStart => 2,
    }
}

fn decode_recovery_stage(value: u8) -> RestorePlanResult<RecoveryStage> {
    match value {
        1 => Ok(RecoveryStage::SafeStart),
        2 => Ok(RecoveryStage::ForensicStart),
        _ => Err(RestorePlanError::PlanCodecInvalidEnum {
            field: "recovery_stage",
            value,
        }),
    }
}

const fn encode_validation_policy(policy: RestoreValidationPolicy) -> u8 {
    match policy {
        RestoreValidationPolicy::Full => 1,
        RestoreValidationPolicy::Minimal => 2,
    }
}

fn decode_validation_policy(value: u8) -> RestorePlanResult<RestoreValidationPolicy> {
    match value {
        1 => Ok(RestoreValidationPolicy::Full),
        2 => Ok(RestoreValidationPolicy::Minimal),
        _ => Err(RestorePlanError::PlanCodecInvalidEnum {
            field: "validation_policy",
            value,
        }),
    }
}

fn decode_binary_flag(value: u8, field: &'static str) -> RestorePlanResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RestorePlanError::PlanCodecInvalidEnum { field, value }),
    }
}

fn validate_decoded_replay_summary(
    replay_segments: &[ReplaySegmentPlanSummaryV0],
    pitr_target_lsn: Lsn,
) -> RestorePlanResult<()> {
    if replay_segments.is_empty() {
        return Ok(());
    }

    let tail_index = replay_segments.len() - 1;
    let mut expected_first_lsn: Option<Lsn> = None;
    for (expected_index, segment) in replay_segments.iter().enumerate() {
        if segment.sequence_index != expected_index {
            return Err(RestorePlanError::ReplaySegmentOrderInvalid);
        }
        if segment.first_lsn.is_zero() {
            return Err(RestorePlanError::PlanCodecValueInvalid {
                field: "first_lsn",
                message: "must not be zero",
            });
        }
        if segment.last_lsn < segment.first_lsn {
            return Err(RestorePlanError::ReplaySegmentChainInvalid);
        }
        if segment.replay_stop_lsn < segment.first_lsn || segment.replay_stop_lsn > segment.last_lsn
        {
            return Err(RestorePlanError::ReplayStopMismatch);
        }

        if let Some(expected_lsn) = expected_first_lsn {
            if segment.first_lsn != expected_lsn {
                return Err(RestorePlanError::ReplaySegmentChainInvalid);
            }
        }

        let is_tail = expected_index == tail_index;
        if segment.contains_pitr_target != is_tail {
            return Err(RestorePlanError::MissingReplayCoverage);
        }
        if is_tail {
            if segment.replay_stop_lsn != pitr_target_lsn {
                return Err(RestorePlanError::ReplayStopMismatch);
            }
        } else if segment.replay_stop_lsn != segment.last_lsn {
            return Err(RestorePlanError::ReplayStopMismatch);
        }

        expected_first_lsn = Some(
            segment
                .last_lsn
                .checked_next()
                .ok_or(RestorePlanError::ReplaySegmentChainInvalid)?,
        );
    }

    Ok(())
}

const fn stage_discriminant(stage: RecoveryStage) -> u64 {
    match stage {
        RecoveryStage::SafeStart => 1,
        RecoveryStage::ForensicStart => 2,
    }
}

const fn policy_discriminant(policy: RestoreValidationPolicy) -> u64 {
    match policy {
        RestoreValidationPolicy::Full => 1,
        RestoreValidationPolicy::Minimal => 2,
    }
}

fn mix_plan_identity(hash: u64, value: u64) -> u64 {
    hash.rotate_left(9)
        .wrapping_mul(0x9E37_79B1_85EB_CA87)
        .wrapping_add(value)
}
