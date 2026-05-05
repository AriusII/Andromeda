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

use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

use crate::{
    backup::{BackupId, BackupManifest},
    Lsn, WalSegmentDescriptor,
};

// ============================================================================
// Helpers
// ============================================================================

fn restore_error(message: impl Into<String>) -> andromeda_core::AndromedaError {
    andromeda_core::AndromedaError::new(andromeda_core::AndromedaErrorKind::Storage, message)
}

// ============================================================================
// Types and Orchestration
// ============================================================================

/// Immutable decision snapshot for a restore attempt.
///
/// Contains all inputs needed to plan replay from backup to a PITR LSN.
/// Orchestration is app-driven (when to execute); this type captures only what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOrchestration {
    /// Durable backup manifest (validated for CRC, version, WAL bounds)
    pub backup_manifest: BackupManifest,

    /// Target LSN for PITR (point-in-time recovery).
    /// Must be within backup's WAL archive range [start, end_inclusive].
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
    /// - PITR target LSN is within backup's WAL archive range
    /// - Audit trace has valid trace ID
    pub fn validate(&self) -> AndromedaResult<()> {
        self.backup_manifest.validate()?;

        // Validate PITR target LSN is in range
        if !self
            .backup_manifest
            .wal_archive
            .contains(self.pitr_target_lsn)
        {
            return Err(restore_error(
                "PITR target LSN must be within backup WAL archive range",
            ));
        }

        self.audit.validate()?;

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

/// Recovery startup stage selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStage {
    /// Standard startup: assume no corruption, replay from checkpoint LSN
    SafeStart,

    /// Forensic startup: detect and report corruption; do not repair in-place
    ForensicStart,
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

// ============================================================================
// Pure Functions (RestorePipeline)
// ============================================================================

/// WAL segment identified for replay during PITR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalSegmentToReplay {
    /// Descriptor of the segment to replay
    pub segment_descriptor: WalSegmentDescriptor,

    /// Position in replay sequence (0 = first segment)
    pub sequence_index: usize,

    /// Whether this segment contains or passes the PITR target LSN
    pub contains_pitr_target: bool,
}

/// Validate restore prerequisites: manifest + PITR target LSN.
///
/// Pure function; no I/O or async.
///
/// Checks:
/// - Manifest identity fields are non-zero
/// - WAL archive range is valid
/// - PITR target LSN is within [start, end_inclusive]
pub fn validate_restore_prerequisites(
    manifest: &BackupManifest,
    pitr_lsn: Lsn,
) -> AndromedaResult<()> {
    // Validate manifest structure
    manifest.validate()?;

    // Validate WAL archive range is valid
    manifest.wal_archive.validate()?;

    // Check PITR target is in range
    if !manifest.wal_archive.contains(pitr_lsn) {
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

    if segments.is_empty() {
        return Err(restore_error("WAL segment list must not be empty"));
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
    // Simple deterministic hash combining manifest fields + WAL bounds
    let mut hash: u64 = 0;

    // Combine backup ID, snapshot ID, created epoch
    hash = hash.wrapping_add(manifest.backup_id.get().wrapping_mul(31));
    hash = hash.wrapping_add(manifest.snapshot.snapshot_id.wrapping_mul(31));
    hash = hash.wrapping_add(manifest.created_epoch.wrapping_mul(31));

    // Combine checkpoint LSN and WAL bounds
    hash = hash.wrapping_add(manifest.snapshot.base_checkpoint_lsn.get().wrapping_mul(31));
    hash = hash.wrapping_add(manifest.wal_archive.start.get().wrapping_mul(31));
    hash = hash.wrapping_add(manifest.wal_archive.end_inclusive.get().wrapping_mul(31));

    // Combine database ID and manifest CRC
    hash = hash.wrapping_add(manifest.database_id.wrapping_mul(31));
    hash = hash.wrapping_add(manifest.manifest_crc as u64);

    hash
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
    fn validate_restore_prerequisites_rejects_pitr_below_range() {
        let manifest = make_test_manifest();
        let pitr_lsn = Lsn::new(1000); // Below archive start

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
