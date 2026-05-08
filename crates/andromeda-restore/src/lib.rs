#![forbid(unsafe_code)]

use std::{error::Error, fmt, path::PathBuf};

pub type RestoreResult<T> = Result<T, RestoreValidationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreValidationError {
    message: String,
}

impl RestoreValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RestoreValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for RestoreValidationError {}

fn restore_error(message: impl Into<String>) -> RestoreValidationError {
    RestoreValidationError::new(message)
}

pub trait RestoreLsn: Copy + Ord + Eq + fmt::Debug {
    fn new(value: u64) -> Self;

    fn get(self) -> u64;

    fn is_zero(self) -> bool {
        self.get() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Lsn(u64);

impl Lsn {
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

impl RestoreLsn for Lsn {
    fn new(value: u64) -> Self {
        Self::new(value)
    }

    fn get(self) -> u64 {
        self.get()
    }
}

pub trait PitrBackupManifest<L: RestoreLsn, Id: Copy> {
    fn backup_manifest_valid(&self) -> bool;
    fn backup_id(&self) -> Id;
    fn database_id(&self) -> u64;
    fn snapshot_id(&self) -> u64;
    fn base_checkpoint_lsn(&self) -> L;
    fn required_wal_start_lsn(&self) -> L;
    fn wal_archive_start(&self) -> L;
    fn wal_archive_end_inclusive(&self) -> L;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrTarget<L = Lsn> {
    pub target_lsn: L,
}

impl<L> PitrTarget<L> {
    pub const fn new(target_lsn: L) -> Self {
        Self { target_lsn }
    }
}

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

    fn into_error(self) -> RestoreValidationError {
        restore_error(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrValidationAccepted<Id = u64, L = Lsn> {
    pub backup_id: Id,
    pub database_id: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: L,
    pub required_wal_start_lsn: L,
    pub wal_archive_start: L,
    pub wal_archive_end_inclusive: L,
    pub target_lsn: L,
    pub replay_skipped: bool,
}

impl<Id, L> PitrValidationAccepted<Id, L> {
    pub const fn requires_wal_replay(&self) -> bool {
        !self.replay_skipped
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitrAuditRecord<Id = u64, L = Lsn> {
    pub backup_id: Id,
    pub database_id: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: L,
    pub required_wal_start_lsn: L,
    pub wal_archive_start: L,
    pub wal_archive_end_inclusive: L,
    pub target_lsn: L,
    pub accepted: bool,
    pub rejection: Option<PitrTargetRejection>,
}

impl<Id: Copy, L: RestoreLsn> PitrAuditRecord<Id, L> {
    fn from_manifest_and_target<M>(
        manifest: &M,
        target: PitrTarget<L>,
        accepted: bool,
        rejection: Option<PitrTargetRejection>,
    ) -> Self
    where
        M: PitrBackupManifest<L, Id>,
    {
        Self {
            backup_id: manifest.backup_id(),
            database_id: manifest.database_id(),
            snapshot_id: manifest.snapshot_id(),
            base_checkpoint_lsn: manifest.base_checkpoint_lsn(),
            required_wal_start_lsn: manifest.required_wal_start_lsn(),
            wal_archive_start: manifest.wal_archive_start(),
            wal_archive_end_inclusive: manifest.wal_archive_end_inclusive(),
            target_lsn: target.target_lsn,
            accepted,
            rejection,
        }
    }
}

pub fn validate_pitr_target<M, Id, L>(
    manifest: &M,
    target: PitrTarget<L>,
) -> RestoreResult<PitrValidationAccepted<Id, L>>
where
    M: PitrBackupManifest<L, Id>,
    Id: Copy,
    L: RestoreLsn,
{
    if !manifest.backup_manifest_valid() {
        return Err(PitrTargetRejection::BackupManifestInvalid.into_error());
    }

    if target.target_lsn.is_zero() {
        return Err(PitrTargetRejection::TargetLsnZero.into_error());
    }

    if manifest.wal_archive_start() > manifest.required_wal_start_lsn() {
        return Err(PitrTargetRejection::WalCoverageMissing.into_error());
    }

    if target.target_lsn < manifest.base_checkpoint_lsn() {
        return Err(PitrTargetRejection::TargetBeforeSnapshot.into_error());
    }

    let replay_skipped = target.target_lsn == manifest.base_checkpoint_lsn();

    if !replay_skipped && target.target_lsn < manifest.required_wal_start_lsn() {
        return Err(PitrTargetRejection::TargetBeforeRequiredWalStart.into_error());
    }

    if target.target_lsn > manifest.wal_archive_end_inclusive() {
        return Err(PitrTargetRejection::TargetBeyondWalRange.into_error());
    }

    Ok(PitrValidationAccepted {
        backup_id: manifest.backup_id(),
        database_id: manifest.database_id(),
        snapshot_id: manifest.snapshot_id(),
        base_checkpoint_lsn: manifest.base_checkpoint_lsn(),
        required_wal_start_lsn: manifest.required_wal_start_lsn(),
        wal_archive_start: manifest.wal_archive_start(),
        wal_archive_end_inclusive: manifest.wal_archive_end_inclusive(),
        target_lsn: target.target_lsn,
        replay_skipped,
    })
}

pub fn validate_pitr_target_with_audit<M, Id, L>(
    manifest: &M,
    target: PitrTarget<L>,
) -> (
    RestoreResult<PitrValidationAccepted<Id, L>>,
    PitrAuditRecord<Id, L>,
)
where
    M: PitrBackupManifest<L, Id>,
    Id: Copy,
    L: RestoreLsn,
{
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreValidationPolicy {
    Full,
    Minimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreArtifactPreflight<Id = u64, L = Lsn, Digest = (), WalArchiveEvidence = ()> {
    pub backup_id: Id,
    pub artifact_root: PathBuf,
    pub manifest_format_version: u16,
    pub validation_policy: RestoreValidationPolicy,
    pub pitr_target_lsn: L,
    pub source_checkpoint_lsn: L,
    pub manifest_digest: Digest,
    pub snapshot_digest: Digest,
    pub wal_archive_evidence: WalArchiveEvidence,
    pub restore_evidence_checksum: u64,
    pub replay_segment_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct TestManifest {
        backup_id: u64,
        database_id: u64,
        snapshot_id: u64,
        base_checkpoint_lsn: Lsn,
        required_wal_start_lsn: Lsn,
        wal_archive_start: Lsn,
        wal_archive_end_inclusive: Lsn,
        valid: bool,
    }

    impl TestManifest {
        fn valid() -> Self {
            Self {
                backup_id: 7,
                database_id: 42,
                snapshot_id: 99,
                base_checkpoint_lsn: Lsn::new(1000),
                required_wal_start_lsn: Lsn::new(1001),
                wal_archive_start: Lsn::new(1001),
                wal_archive_end_inclusive: Lsn::new(2000),
                valid: true,
            }
        }
    }

    impl PitrBackupManifest<Lsn, u64> for TestManifest {
        fn backup_manifest_valid(&self) -> bool {
            self.valid
        }

        fn backup_id(&self) -> u64 {
            self.backup_id
        }

        fn database_id(&self) -> u64 {
            self.database_id
        }

        fn snapshot_id(&self) -> u64 {
            self.snapshot_id
        }

        fn base_checkpoint_lsn(&self) -> Lsn {
            self.base_checkpoint_lsn
        }

        fn required_wal_start_lsn(&self) -> Lsn {
            self.required_wal_start_lsn
        }

        fn wal_archive_start(&self) -> Lsn {
            self.wal_archive_start
        }

        fn wal_archive_end_inclusive(&self) -> Lsn {
            self.wal_archive_end_inclusive
        }
    }

    #[test]
    fn pitr_accepts_snapshot_base_without_wal_replay() {
        let accepted =
            validate_pitr_target(&TestManifest::valid(), PitrTarget::new(Lsn::new(1000)))
                .expect("snapshot base target should be valid");

        assert!(accepted.replay_skipped);
        assert!(!accepted.requires_wal_replay());
    }

    #[test]
    fn pitr_rejects_archive_that_misses_required_wal_start() {
        let mut manifest = TestManifest::valid();
        manifest.wal_archive_start = Lsn::new(1002);

        let (result, audit) =
            validate_pitr_target_with_audit(&manifest, PitrTarget::new(Lsn::new(1500)));

        assert!(result.is_err());
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::WalCoverageMissing)
        );
    }

    #[test]
    fn pitr_rejects_target_before_required_wal_start() {
        let mut manifest = TestManifest::valid();
        manifest.required_wal_start_lsn = Lsn::new(1005);
        manifest.wal_archive_start = Lsn::new(900);

        let (result, audit) =
            validate_pitr_target_with_audit(&manifest, PitrTarget::new(Lsn::new(1002)));

        assert!(result.is_err());
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::TargetBeforeRequiredWalStart)
        );
    }

    #[test]
    fn pitr_rejects_target_beyond_archive_end() {
        let (result, audit) = validate_pitr_target_with_audit(
            &TestManifest::valid(),
            PitrTarget::new(Lsn::new(2001)),
        );

        assert!(result.is_err());
        assert_eq!(
            audit.rejection,
            Some(PitrTargetRejection::TargetBeyondWalRange)
        );
    }
}
