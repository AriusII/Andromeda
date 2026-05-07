use std::path::PathBuf;

use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

use crate::{
    Lsn,
    backup::{BackupArtifactDigest, BackupId, BackupManifest, BackupWalArchiveEvidence},
};

use super::{
    checksum::compute_restore_checksum,
    error::restore_error,
    validation::{
        validate_preflight_matches_orchestration, validate_restore_prerequisites,
        validate_stage_policy,
    },
};

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
    ) -> AndromedaResult<()> {
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
