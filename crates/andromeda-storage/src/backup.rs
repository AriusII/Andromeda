//! V0 backup metadata and Point-In-Time Recovery (PITR) target validation.
//!
//! Doctrine reminders enforced here:
//!
//! * `RAM is never truth` — a backup manifest only references durable cold
//!   snapshot identity and a WAL LSN range. No runtime-only or RAM-resident
//!   state is part of the contract.
//! * `visible commit == durable WAL evidence` — PITR can only target an LSN
//!   that is fully covered by a durable WAL range whose start anchors into
//!   the snapshot's required WAL start.
//! * Backup/PITR must not depend on GPU or runtime-only state. The structures
//!   here intentionally carry no GPU, RAM-page, or pipeline-execution fields.
//! * This module models *backup metadata, snapshot/WAL coverage, and PITR
//!   target validation only*. It is intentionally separate from
//!   single-primary HA quorum, failover, or replica role negotiation, which
//!   live in `crate::write_ahead_log::shipping` and future HA modules.
//!
//! No unsafe, no gRPC, no SQL, no runtime JSON.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion};
use andromeda_observe::TraceId;

use crate::{DatabaseManifest, Lsn, WAL_FORMAT_VERSION};

pub const BACKUP_PHYSICAL_PLAN_VERSION_V0: u16 = 1;
pub const BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0: u16 = 1;

/// Stable identifier of a backup artifact.
///
/// The numeric id is opaque; equality is by `(database_id, backup_id)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BackupId(u64);

impl BackupId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Inclusive LSN range covered by a durable WAL archive that ships alongside
/// a cold snapshot. Both endpoints are inclusive and `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalArchiveRange {
    pub start: Lsn,
    pub end_inclusive: Lsn,
}

impl WalArchiveRange {
    pub const fn new(start: Lsn, end_inclusive: Lsn) -> Self {
        Self {
            start,
            end_inclusive,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.start.is_zero() {
            return Err(backup_error("WAL archive start LSN must not be zero"));
        }
        if self.end_inclusive < self.start {
            return Err(backup_error(
                "WAL archive end LSN must not precede start LSN",
            ));
        }
        Ok(())
    }

    /// True if `lsn` falls within `[start, end_inclusive]`.
    pub fn contains(self, lsn: Lsn) -> bool {
        lsn >= self.start && lsn <= self.end_inclusive
    }
}

/// Cold snapshot boundary referenced by a backup manifest.
///
/// Mirrors the durable identity in [`DatabaseManifest`] without depending on
/// any RAM-resident derived state. `base_checkpoint_lsn` is the highest LSN
/// included in the snapshot itself; `required_wal_start_lsn` is the first
/// LSN the accompanying WAL archive must cover for replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdSnapshotBoundary {
    pub snapshot_id: u64,
    pub snapshot_descriptor_hash: [u8; 32],
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
}

impl ColdSnapshotBoundary {
    pub fn from_manifest(manifest: &DatabaseManifest, snapshot_descriptor_hash: [u8; 32]) -> Self {
        Self {
            snapshot_id: manifest.snapshot_id,
            snapshot_descriptor_hash,
            base_checkpoint_lsn: manifest.base_checkpoint_lsn,
            required_wal_start_lsn: manifest.required_wal_start_lsn,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.snapshot_id == 0 {
            return Err(backup_error("snapshot id must not be zero"));
        }
        if self.snapshot_descriptor_hash == [0; 32] {
            return Err(backup_error("snapshot descriptor hash must not be zero"));
        }
        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(backup_error(
                "snapshot required WAL start LSN must not precede base checkpoint LSN",
            ));
        }
        Ok(())
    }
}

/// Durable backup manifest binding a cold snapshot to a contiguous WAL
/// archive range. This is the *only* payload used for PITR validation;
/// nothing in here is allowed to come from RAM or GPU state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupManifest {
    pub backup_id: BackupId,
    pub database_id: u64,
    pub created_epoch: u64,
    pub snapshot: ColdSnapshotBoundary,
    pub wal_archive: WalArchiveRange,
    pub manifest_crc: u32,
}

impl BackupManifest {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.backup_id.is_zero() {
            return Err(backup_error("backup id must not be zero"));
        }
        if self.database_id == 0 {
            return Err(backup_error("backup database id must not be zero"));
        }
        if self.created_epoch == 0 {
            return Err(backup_error("backup created epoch must not be zero"));
        }
        if self.manifest_crc == 0 {
            return Err(backup_error("backup manifest CRC must not be zero"));
        }
        self.snapshot.validate()?;
        self.wal_archive.validate()?;

        // Some backups may ship only the snapshot (WAL range degenerate at
        // the checkpoint). Whatever the case, the WAL end must be at least
        // the snapshot's checkpoint LSN, otherwise the snapshot itself is
        // not even minimally re-anchorable.
        if self.wal_archive.end_inclusive < self.snapshot.base_checkpoint_lsn {
            return Err(backup_error(
                "backup WAL archive end must not precede snapshot base checkpoint LSN",
            ));
        }

        // Note: the stronger invariant
        //     `wal_archive.start <= snapshot.required_wal_start_lsn`
        // is *replay-time* coverage, not structural identity. It is
        // checked by [`validate_pitr_target`] so PITR consumers see a
        // dedicated [`PitrTargetRejection::WalCoverageMissing`] reason
        // rather than a generic structural error.

        Ok(())
    }

    /// Earliest LSN that PITR may target against this backup. Equal to the
    /// snapshot's base checkpoint LSN: the snapshot itself is the cold
    /// floor, and replay can only move forward from there.
    pub const fn earliest_pitr_target(&self) -> Lsn {
        self.snapshot.base_checkpoint_lsn
    }

    /// Latest LSN that PITR may target against this backup. Equal to the
    /// last LSN durably covered by the WAL archive.
    pub const fn latest_pitr_target(&self) -> Lsn {
        self.wal_archive.end_inclusive
    }
}

/// Digest/checksum identity for a durable physical backup artifact.
///
/// This deliberately contains only durable byte evidence: no in-memory WAL
/// handles, no EventSink-derived truth, no GPU/runtime pipeline fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupArtifactDigest {
    pub sha256: [u8; 32],
    pub crc64: u64,
    pub byte_len: u64,
}

impl BackupArtifactDigest {
    pub fn validate(&self, label: &str) -> AndromedaResult<()> {
        if self.sha256 == [0; 32] {
            return Err(backup_error(format!("{label} SHA-256 digest must not be zero")));
        }
        if self.crc64 == 0 {
            return Err(backup_error(format!("{label} CRC64 must not be zero")));
        }
        if self.byte_len == 0 {
            return Err(backup_error(format!("{label} byte length must not be zero")));
        }
        Ok(())
    }
}

/// Compatibility tuple recorded with every physical backup plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupCompatibility {
    pub plan_version: u16,
    pub catalog_version: CatalogVersion,
    pub storage_format_version: u16,
    pub wal_format_version: u16,
}

impl BackupCompatibility {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.plan_version != BACKUP_PHYSICAL_PLAN_VERSION_V0 {
            return Err(backup_error("unsupported backup physical plan version"));
        }
        if self.catalog_version.get() == 0 {
            return Err(backup_error("backup catalog version must not be zero"));
        }
        if self.storage_format_version != BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0 {
            return Err(backup_error("unsupported backup storage format version"));
        }
        if self.wal_format_version != WAL_FORMAT_VERSION {
            return Err(backup_error("unsupported backup WAL format version"));
        }
        Ok(())
    }
}

/// Durable cold-snapshot artifact bound to the backup manifest identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupColdSnapshotArtifact {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub snapshot_descriptor_hash: [u8; 32],
    pub manifest_crc: u32,
    pub artifact: BackupArtifactDigest,
}

impl BackupColdSnapshotArtifact {
    pub fn validate_against(&self, manifest: &BackupManifest) -> AndromedaResult<()> {
        if self.database_id != manifest.database_id {
            return Err(backup_error(
                "cold snapshot artifact database id must match backup manifest",
            ));
        }
        if self.manifest_version == 0 {
            return Err(backup_error(
                "cold snapshot artifact manifest version must not be zero",
            ));
        }
        if self.snapshot_id != manifest.snapshot.snapshot_id {
            return Err(backup_error(
                "cold snapshot artifact id must match backup manifest snapshot",
            ));
        }
        if self.snapshot_descriptor_hash != manifest.snapshot.snapshot_descriptor_hash {
            return Err(backup_error(
                "cold snapshot artifact descriptor hash must match backup manifest",
            ));
        }
        if self.manifest_crc == 0 {
            return Err(backup_error(
                "cold snapshot artifact manifest CRC must not be zero",
            ));
        }
        self.artifact.validate("cold snapshot artifact")
    }
}

/// Durable WAL segment artifact participating in the backup WAL archive range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupWalSegmentArtifact {
    pub segment_id: u64,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub base_previous_lsn: Option<Lsn>,
    pub record_count: u64,
    pub wal_format_version: u16,
    pub artifact: BackupArtifactDigest,
}

impl BackupWalSegmentArtifact {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_id == 0 {
            return Err(backup_error("backup WAL segment id must not be zero"));
        }
        if self.wal_format_version != WAL_FORMAT_VERSION {
            return Err(backup_error("backup WAL segment format version mismatch"));
        }
        if self.record_count == 0 {
            return Err(backup_error("backup WAL segment record count must not be zero"));
        }
        if self.first_lsn.is_zero() || self.last_lsn.is_zero() {
            return Err(backup_error("backup WAL segment LSN bounds must not be zero"));
        }
        if self.last_lsn < self.first_lsn {
            return Err(backup_error("backup WAL segment last LSN precedes first LSN"));
        }
        match self.base_previous_lsn {
            Some(previous) => {
                if previous.try_next()? != self.first_lsn {
                    return Err(backup_error(
                        "backup WAL segment base previous LSN must chain to first LSN",
                    ));
                }
            }
            None => {
                if self.first_lsn != Lsn::new(1) {
                    return Err(backup_error(
                        "backup WAL segment without base previous LSN must start at LSN 1",
                    ));
                }
            }
        }
        self.artifact.validate("backup WAL segment artifact")
    }
}

/// Exact durable artifact set expected from F5 physical backup execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPhysicalArtifactSet {
    pub backup_manifest: BackupArtifactDigest,
    pub cold_snapshot: BackupColdSnapshotArtifact,
    pub wal_segments: Vec<BackupWalSegmentArtifact>,
}

impl BackupPhysicalArtifactSet {
    pub fn validate_against(&self, manifest: &BackupManifest) -> AndromedaResult<()> {
        self.backup_manifest.validate("backup manifest artifact")?;
        self.cold_snapshot.validate_against(manifest)?;
        validate_wal_segment_chain(manifest.wal_archive, &self.wal_segments)
    }

    fn wal_archive_bytes(&self) -> AndromedaResult<u64> {
        let mut total = 0_u64;
        for segment in &self.wal_segments {
            total = total
                .checked_add(segment.artifact.byte_len)
                .ok_or_else(|| backup_error("backup WAL archive byte length overflows u64"))?;
        }
        Ok(total)
    }
}

/// Resource guardrails for a physical backup. Observed bytes are derived from
/// durable artifact sizes, never from RAM page cache state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupResourceBounds {
    pub max_snapshot_bytes: u64,
    pub max_wal_archive_bytes: u64,
    pub max_wal_segment_count: usize,
    pub observed_snapshot_bytes: u64,
    pub observed_wal_archive_bytes: u64,
    pub observed_wal_segment_count: usize,
}

impl BackupResourceBounds {
    pub fn validate_against(&self, artifacts: &BackupPhysicalArtifactSet) -> AndromedaResult<()> {
        if self.max_snapshot_bytes == 0
            || self.max_wal_archive_bytes == 0
            || self.max_wal_segment_count == 0
        {
            return Err(backup_error("backup resource maxima must not be zero"));
        }
        if self.observed_snapshot_bytes != artifacts.cold_snapshot.artifact.byte_len {
            return Err(backup_error(
                "backup observed snapshot bytes must match cold snapshot artifact",
            ));
        }
        let wal_bytes = artifacts.wal_archive_bytes()?;
        if self.observed_wal_archive_bytes != wal_bytes {
            return Err(backup_error(
                "backup observed WAL bytes must match WAL segment artifacts",
            ));
        }
        if self.observed_wal_segment_count != artifacts.wal_segments.len() {
            return Err(backup_error(
                "backup observed WAL segment count must match artifact list",
            ));
        }
        if self.observed_snapshot_bytes > self.max_snapshot_bytes {
            return Err(backup_error("backup snapshot bytes exceed resource bound"));
        }
        if self.observed_wal_archive_bytes > self.max_wal_archive_bytes {
            return Err(backup_error("backup WAL bytes exceed resource bound"));
        }
        if self.observed_wal_segment_count > self.max_wal_segment_count {
            return Err(backup_error("backup WAL segment count exceeds resource bound"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupIncompleteTransactionPolicy {
    DiscardOnRecovery,
}

/// Durable boundary proof for incomplete transactions at the backup WAL end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupIncompleteTransactionBoundary {
    pub durable_wal_end_lsn: Lsn,
    pub highest_committed_lsn: Lsn,
    pub incomplete_transaction_count: u64,
    pub policy: BackupIncompleteTransactionPolicy,
}

impl BackupIncompleteTransactionBoundary {
    pub fn validate_against(&self, manifest: &BackupManifest) -> AndromedaResult<()> {
        if self.durable_wal_end_lsn != manifest.wal_archive.end_inclusive {
            return Err(backup_error(
                "backup incomplete transaction boundary must end at WAL archive end",
            ));
        }
        if self.highest_committed_lsn > self.durable_wal_end_lsn {
            return Err(backup_error(
                "backup highest committed LSN must not exceed durable WAL end",
            ));
        }
        if self.incomplete_transaction_count > 0
            && self.policy != BackupIncompleteTransactionPolicy::DiscardOnRecovery
        {
            return Err(backup_error(
                "backup incomplete transactions must be marked discard-on-recovery",
            ));
        }
        Ok(())
    }
}

/// Audit fields that must accompany a backup decision when execution wiring is
/// added. Identity values are hashes/ids only; no ad hoc SQL or runtime JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupAuditTraceFields {
    pub trace_id: TraceId,
    pub requested_epoch: u64,
    pub completed_epoch: u64,
    pub actor_id_hash: [u8; 32],
    pub decision_reason_hash: [u8; 32],
}

impl BackupAuditTraceFields {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_zero() {
            return Err(backup_error("backup audit trace id must not be zero"));
        }
        if self.requested_epoch == 0 || self.completed_epoch == 0 {
            return Err(backup_error("backup audit epochs must not be zero"));
        }
        if self.completed_epoch < self.requested_epoch {
            return Err(backup_error(
                "backup audit completed epoch must not precede requested epoch",
            ));
        }
        if self.actor_id_hash == [0; 32] {
            return Err(backup_error("backup audit actor hash must not be zero"));
        }
        if self.decision_reason_hash == [0; 32] {
            return Err(backup_error(
                "backup audit decision reason hash must not be zero",
            ));
        }
        Ok(())
    }
}

/// F5 physical backup execution plan guard. It validates the durable artifact
/// list and boundaries only; it does not copy files, restore, replay PITR, ship
/// WAL, or consult runtime memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPhysicalPlan {
    pub manifest: BackupManifest,
    pub compatibility: BackupCompatibility,
    pub artifacts: BackupPhysicalArtifactSet,
    pub resource_bounds: BackupResourceBounds,
    pub incomplete_transaction_boundary: BackupIncompleteTransactionBoundary,
    /// Any observed corruption boundary makes the backup invalid. Corrupt WAL
    /// may still be forensic evidence, but it is not a valid backup artifact.
    pub corruption_boundary_lsn: Option<Lsn>,
    pub audit: BackupAuditTraceFields,
}

impl BackupPhysicalPlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.manifest.validate()?;
        self.compatibility.validate()?;
        self.artifacts.validate_against(&self.manifest)?;
        self.resource_bounds.validate_against(&self.artifacts)?;
        self.incomplete_transaction_boundary
            .validate_against(&self.manifest)?;
        if self.corruption_boundary_lsn.is_some() {
            return Err(backup_error(
                "backup physical plan must reject corrupt WAL or snapshot artifacts",
            ));
        }
        self.audit.validate()
    }
}

fn validate_wal_segment_chain(
    archive: WalArchiveRange,
    segments: &[BackupWalSegmentArtifact],
) -> AndromedaResult<()> {
    archive.validate()?;
    let Some(first) = segments.first() else {
        return Err(backup_error("backup WAL archive must list at least one segment"));
    };
    if first.first_lsn != archive.start {
        return Err(backup_error(
            "backup WAL first segment must start at archive start LSN",
        ));
    }

    let mut expected_first = archive.start;
    let mut previous_last = None;
    for (index, segment) in segments.iter().enumerate() {
        segment.validate()?;
        if segment.first_lsn != expected_first {
            return Err(backup_error("backup WAL segment chain has an LSN gap"));
        }
        if segment.base_previous_lsn != previous_last {
            return Err(backup_error(
                "backup WAL segment base previous LSN does not chain to prior segment",
            ));
        }
        if segments[..index]
            .iter()
            .any(|previous| previous.segment_id == segment.segment_id)
        {
            return Err(backup_error("backup WAL segment ids must not repeat"));
        }
        previous_last = Some(segment.last_lsn);
        expected_first = segment.last_lsn.try_next()?;
    }

    let last = segments
        .last()
        .ok_or_else(|| backup_error("backup WAL archive must list at least one segment"))?;
    if last.last_lsn != archive.end_inclusive {
        return Err(backup_error(
            "backup WAL last segment must end at archive end LSN",
        ));
    }
    Ok(())
}

/// PITR target requested by an operator. Carries only the durable LSN to
/// recover to; no RAM-derived hints, no runtime handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrTarget {
    pub target_lsn: Lsn,
}

impl PitrTarget {
    pub const fn new(target_lsn: Lsn) -> Self {
        Self { target_lsn }
    }
}

/// Categorical reason a PITR target was rejected. Each variant maps to a
/// distinct operator action so audit/trace consumers can route alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitrTargetRejection {
    /// The bound backup manifest itself failed validation.
    BackupManifestInvalid,
    /// The target LSN is zero; PITR requires a positive LSN.
    TargetLsnZero,
    /// The target LSN is below the snapshot's base checkpoint LSN; the
    /// snapshot is the cold floor and we cannot rewind below it.
    TargetBeforeSnapshot,
    /// The target LSN is above the WAL archive's last covered LSN.
    TargetBeyondWalRange,
    /// The WAL archive does not start at or before the snapshot's required
    /// WAL start LSN, so even an in-range target cannot be replayed onto
    /// the snapshot floor.
    WalCoverageMissing,
}

impl PitrTargetRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BackupManifestInvalid => "backup manifest failed validation",
            Self::TargetLsnZero => "PITR target LSN must not be zero",
            Self::TargetBeforeSnapshot => {
                "PITR target LSN is below the snapshot base checkpoint LSN"
            }
            Self::TargetBeyondWalRange => "PITR target LSN is above the WAL archive end LSN",
            Self::WalCoverageMissing => {
                "WAL archive does not anchor into the snapshot required WAL start LSN"
            }
        }
    }

    fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

/// Successful PITR validation outcome with audit-friendly fields suitable
/// for emitting into the observe trace stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrValidationAccepted {
    pub backup_id: BackupId,
    pub database_id: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub wal_archive_start: Lsn,
    pub wal_archive_end_inclusive: Lsn,
    pub target_lsn: Lsn,
    /// True when the target equals the snapshot floor and no WAL replay is
    /// needed.
    pub replay_skipped: bool,
}

impl PitrValidationAccepted {
    /// Convenience flag mirroring `replay_skipped` for symmetry.
    pub const fn requires_wal_replay(&self) -> bool {
        !self.replay_skipped
    }
}

/// Audit record describing a PITR validation attempt — accepted or
/// rejected. All fields come from durable inputs only, so this record can
/// be safely persisted alongside backup metadata or shipped to the observe
/// crate without introducing RAM-only truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrAuditRecord {
    pub backup_id: BackupId,
    pub database_id: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub wal_archive_start: Lsn,
    pub wal_archive_end_inclusive: Lsn,
    pub target_lsn: Lsn,
    pub accepted: bool,
    pub rejection: Option<PitrTargetRejection>,
}

impl PitrAuditRecord {
    fn from_manifest_and_target(
        manifest: &BackupManifest,
        target: PitrTarget,
        accepted: bool,
        rejection: Option<PitrTargetRejection>,
    ) -> Self {
        Self {
            backup_id: manifest.backup_id,
            database_id: manifest.database_id,
            snapshot_id: manifest.snapshot.snapshot_id,
            base_checkpoint_lsn: manifest.snapshot.base_checkpoint_lsn,
            required_wal_start_lsn: manifest.snapshot.required_wal_start_lsn,
            wal_archive_start: manifest.wal_archive.start,
            wal_archive_end_inclusive: manifest.wal_archive.end_inclusive,
            target_lsn: target.target_lsn,
            accepted,
            rejection,
        }
    }
}

/// Validate a PITR target against a backup manifest.
///
/// Returns `Ok(PitrValidationAccepted)` when:
///
/// 1. The backup manifest itself is structurally valid.
/// 2. `target.target_lsn` is non-zero.
/// 3. `target.target_lsn >= snapshot.base_checkpoint_lsn`.
/// 4. `target.target_lsn <= wal_archive.end_inclusive`.
/// 5. `wal_archive.start <= snapshot.required_wal_start_lsn` so replay
///    anchors into the snapshot floor.
///
/// All other paths return a typed [`PitrTargetRejection`] wrapped in an
/// [`AndromedaError`] with kind [`AndromedaErrorKind::Storage`].
pub fn validate_pitr_target(
    manifest: &BackupManifest,
    target: PitrTarget,
) -> AndromedaResult<PitrValidationAccepted> {
    if manifest.validate().is_err() {
        return Err(PitrTargetRejection::BackupManifestInvalid.into_error());
    }

    if target.target_lsn.is_zero() {
        return Err(PitrTargetRejection::TargetLsnZero.into_error());
    }

    // WAL coverage anchoring: validated before checking target position so
    // operators see the upstream cause first.
    if manifest.wal_archive.start > manifest.snapshot.required_wal_start_lsn {
        return Err(PitrTargetRejection::WalCoverageMissing.into_error());
    }

    if target.target_lsn < manifest.snapshot.base_checkpoint_lsn {
        return Err(PitrTargetRejection::TargetBeforeSnapshot.into_error());
    }

    if target.target_lsn > manifest.wal_archive.end_inclusive {
        return Err(PitrTargetRejection::TargetBeyondWalRange.into_error());
    }

    let replay_skipped = target.target_lsn == manifest.snapshot.base_checkpoint_lsn;

    Ok(PitrValidationAccepted {
        backup_id: manifest.backup_id,
        database_id: manifest.database_id,
        snapshot_id: manifest.snapshot.snapshot_id,
        base_checkpoint_lsn: manifest.snapshot.base_checkpoint_lsn,
        required_wal_start_lsn: manifest.snapshot.required_wal_start_lsn,
        wal_archive_start: manifest.wal_archive.start,
        wal_archive_end_inclusive: manifest.wal_archive.end_inclusive,
        target_lsn: target.target_lsn,
        replay_skipped,
    })
}

/// Run [`validate_pitr_target`] and produce a [`PitrAuditRecord`] regardless
/// of outcome. Useful for forensic startup and PITR runbook tooling that
/// must always record an audit trail for the attempt.
pub fn validate_pitr_target_with_audit(
    manifest: &BackupManifest,
    target: PitrTarget,
) -> (AndromedaResult<PitrValidationAccepted>, PitrAuditRecord) {
    match validate_pitr_target(manifest, target) {
        Ok(accepted) => {
            let audit = PitrAuditRecord::from_manifest_and_target(manifest, target, true, None);
            (Ok(accepted), audit)
        }
        Err(err) => {
            // Recover the rejection reason from the error message so callers
            // get a structured audit even when the manifest itself fails.
            let rejection = classify_rejection(err.message());
            let audit =
                PitrAuditRecord::from_manifest_and_target(manifest, target, false, Some(rejection));
            (Err(err), audit)
        }
    }
}

fn classify_rejection(message: &str) -> PitrTargetRejection {
    if message == PitrTargetRejection::BackupManifestInvalid.as_str() {
        PitrTargetRejection::BackupManifestInvalid
    } else if message == PitrTargetRejection::TargetLsnZero.as_str() {
        PitrTargetRejection::TargetLsnZero
    } else if message == PitrTargetRejection::TargetBeforeSnapshot.as_str() {
        PitrTargetRejection::TargetBeforeSnapshot
    } else if message == PitrTargetRejection::TargetBeyondWalRange.as_str() {
        PitrTargetRejection::TargetBeyondWalRange
    } else if message == PitrTargetRejection::WalCoverageMissing.as_str() {
        PitrTargetRejection::WalCoverageMissing
    } else {
        PitrTargetRejection::BackupManifestInvalid
    }
}

fn backup_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_boundary() -> ColdSnapshotBoundary {
        ColdSnapshotBoundary {
            snapshot_id: 7,
            snapshot_descriptor_hash: [9; 32],
            base_checkpoint_lsn: Lsn::new(100),
            required_wal_start_lsn: Lsn::new(101),
        }
    }

    fn manifest() -> BackupManifest {
        BackupManifest {
            backup_id: BackupId::new(1),
            database_id: 42,
            created_epoch: 5,
            snapshot: snapshot_boundary(),
            wal_archive: WalArchiveRange::new(Lsn::new(101), Lsn::new(200)),
            manifest_crc: 0xdead_beef,
        }
    }

    #[test]
    fn manifest_validates_when_well_formed() {
        manifest().validate().unwrap();
    }

    #[test]
    fn manifest_accepts_wal_archive_starting_after_snapshot_anchor_structurally() {
        // Structural validation does NOT catch missing replay anchoring;
        // that is a PITR-time check so the rejection reason is precise.
        let mut m = manifest();
        m.wal_archive = WalArchiveRange::new(Lsn::new(150), Lsn::new(200));
        m.validate().unwrap();
    }

    #[test]
    fn pitr_accepts_target_inside_wal_range() {
        let m = manifest();
        let target = PitrTarget::new(Lsn::new(150));
        let accepted = validate_pitr_target(&m, target).unwrap();
        assert_eq!(accepted.target_lsn, Lsn::new(150));
        assert!(!accepted.replay_skipped);
        assert!(accepted.requires_wal_replay());
    }

    #[test]
    fn pitr_accepts_target_at_snapshot_floor_without_replay() {
        let m = manifest();
        let target = PitrTarget::new(Lsn::new(100));
        let accepted = validate_pitr_target(&m, target).unwrap();
        assert!(accepted.replay_skipped);
        assert!(!accepted.requires_wal_replay());
    }

    #[test]
    fn pitr_rejects_target_before_snapshot_floor() {
        let m = manifest();
        let target = PitrTarget::new(Lsn::new(50));
        let (result, audit) = validate_pitr_target_with_audit(&m, target);
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
        assert!(!audit.accepted);
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::TargetBeforeSnapshot)
        );
        assert_eq!(audit.target_lsn, Lsn::new(50));
        assert_eq!(audit.snapshot_id, 7);
    }

    #[test]
    fn pitr_rejects_target_beyond_wal_range() {
        let m = manifest();
        let target = PitrTarget::new(Lsn::new(201));
        let (result, audit) = validate_pitr_target_with_audit(&m, target);
        assert!(result.is_err());
        assert!(!audit.accepted);
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::TargetBeyondWalRange)
        );
    }

    #[test]
    fn pitr_rejects_when_wal_archive_does_not_anchor_into_snapshot() {
        // WAL archive starts strictly after the snapshot's required WAL
        // start, so replay cannot anchor onto the cold snapshot floor.
        // Structural manifest validation passes (it intentionally does not
        // enforce replay-time coverage), so the PITR validator surfaces
        // the dedicated [`PitrTargetRejection::WalCoverageMissing`] reason.
        let mut m = manifest();
        m.wal_archive = WalArchiveRange::new(Lsn::new(150), Lsn::new(200));
        let target = PitrTarget::new(Lsn::new(160));
        let (result, audit) = validate_pitr_target_with_audit(&m, target);
        assert!(result.is_err());
        assert!(!audit.accepted);
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::WalCoverageMissing)
        );
        assert_eq!(audit.target_lsn, Lsn::new(160));
        assert_eq!(audit.wal_archive_start, Lsn::new(150));
        assert_eq!(audit.required_wal_start_lsn, Lsn::new(101));
    }

    #[test]
    fn pitr_rejects_zero_target_lsn() {
        let m = manifest();
        let target = PitrTarget::new(Lsn::ZERO);
        let (result, audit) = validate_pitr_target_with_audit(&m, target);
        assert!(result.is_err());
        assert_eq!(audit.rejection, Some(PitrTargetRejection::TargetLsnZero));
    }

    #[test]
    fn pitr_rejects_invalid_manifest() {
        let mut m = manifest();
        m.backup_id = BackupId::new(0);
        let target = PitrTarget::new(Lsn::new(150));
        let (result, audit) = validate_pitr_target_with_audit(&m, target);
        assert!(result.is_err());
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::BackupManifestInvalid)
        );
    }

    #[test]
    fn cold_snapshot_boundary_can_be_built_from_database_manifest() {
        let dm = DatabaseManifest {
            database_id: 11,
            manifest_version: 2,
            snapshot_id: 7,
            base_checkpoint_lsn: Lsn::new(100),
            required_wal_start_lsn: Lsn::new(101),
            previous_manifest_hash: [3; 32],
            manifest_crc: 0xa5a5_a5a5,
        };
        let boundary = ColdSnapshotBoundary::from_manifest(&dm, [9; 32]);
        boundary.validate().unwrap();
        assert_eq!(boundary, snapshot_boundary());
    }
}
