//! Backup manifest, physical plan, and PITR validation contracts.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

use super::artifacts::BackupWalSegmentArtifact;
use super::helpers::backup_error;
use super::types::{BackupId, ColdSnapshotBoundary, WalArchiveRange};

/// Durable backup manifest binding a cold snapshot to a contiguous WAL archive range.
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

        if self.wal_archive.end_inclusive < self.snapshot.base_checkpoint_lsn {
            return Err(backup_error(
                "backup WAL archive end must not precede snapshot base checkpoint LSN",
            ));
        }

        Ok(())
    }

    pub const fn earliest_pitr_target(&self) -> Lsn {
        self.snapshot.base_checkpoint_lsn
    }

    pub const fn latest_pitr_target(&self) -> Lsn {
        self.wal_archive.end_inclusive
    }
}

pub(super) fn validate_wal_segment_chain(
    archive: WalArchiveRange,
    segments: &[BackupWalSegmentArtifact],
) -> AndromedaResult<()> {
    archive.validate()?;
    let Some(first) = segments.first() else {
        return Err(backup_error(
            "backup WAL archive must list at least one segment",
        ));
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

/// PITR target requested by an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrTarget {
    pub target_lsn: Lsn,
}

impl PitrTarget {
    pub const fn new(target_lsn: Lsn) -> Self {
        Self { target_lsn }
    }
}

/// Categorical reason a PITR target was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitrTargetRejection {
    BackupManifestInvalid,
    TargetLsnZero,
    TargetBeforeSnapshot,
    TargetBeforeRequiredWalStart,
    TargetBeyondWalRange,
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
            Self::TargetBeforeRequiredWalStart => {
                "PITR target LSN is below the snapshot required WAL start LSN"
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

/// Successful PITR validation outcome with audit-friendly durable fields.
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
    pub replay_skipped: bool,
}

impl PitrValidationAccepted {
    pub const fn requires_wal_replay(&self) -> bool {
        !self.replay_skipped
    }
}

/// Audit record describing a PITR validation attempt.
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

    if manifest.wal_archive.start > manifest.snapshot.required_wal_start_lsn {
        return Err(PitrTargetRejection::WalCoverageMissing.into_error());
    }

    if target.target_lsn < manifest.snapshot.base_checkpoint_lsn {
        return Err(PitrTargetRejection::TargetBeforeSnapshot.into_error());
    }

    let replay_skipped = target.target_lsn == manifest.snapshot.base_checkpoint_lsn;

    if !replay_skipped && target.target_lsn < manifest.snapshot.required_wal_start_lsn {
        return Err(PitrTargetRejection::TargetBeforeRequiredWalStart.into_error());
    }

    if target.target_lsn > manifest.wal_archive.end_inclusive {
        return Err(PitrTargetRejection::TargetBeyondWalRange.into_error());
    }

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
    } else if message == PitrTargetRejection::TargetBeforeRequiredWalStart.as_str() {
        PitrTargetRejection::TargetBeforeRequiredWalStart
    } else if message == PitrTargetRejection::TargetBeyondWalRange.as_str() {
        PitrTargetRejection::TargetBeyondWalRange
    } else if message == PitrTargetRejection::WalCoverageMissing.as_str() {
        PitrTargetRejection::WalCoverageMissing
    } else {
        PitrTargetRejection::BackupManifestInvalid
    }
}
