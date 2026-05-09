use andromeda_error::AndromedaResult;
pub use andromeda_recovery::FileWalStartupRecoveryV0;
use andromeda_recovery::{FileWalRecoveryReportV0, RecoveryManifestView};
use std::path::Path;

use crate::{ConceptualRedoPlan, DatabaseManifest, Lsn, StartupMode};

pub fn recover_from_file_wal(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<ConceptualRedoPlan> {
    andromeda_recovery::recover_from_file_wal(
        &StorageFileWalRecoveryManifest { manifest },
        startup_mode,
        path,
    )
}

pub fn plan_file_wal_startup_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
    forensic_report_attached: bool,
) -> AndromedaResult<FileWalStartupRecoveryV0> {
    andromeda_recovery::plan_file_wal_startup_recovery_v0(
        &StorageFileWalRecoveryManifest { manifest },
        startup_mode,
        path,
        forensic_report_attached,
    )
}

pub fn report_file_wal_recovery_v0(
    manifest: &DatabaseManifest,
    startup_mode: StartupMode,
    path: impl AsRef<Path>,
) -> AndromedaResult<FileWalRecoveryReportV0> {
    andromeda_recovery::report_file_wal_recovery_v0(
        &StorageFileWalRecoveryManifest { manifest },
        startup_mode,
        path,
    )
}

struct StorageFileWalRecoveryManifest<'a> {
    manifest: &'a DatabaseManifest,
}

impl RecoveryManifestView for StorageFileWalRecoveryManifest<'_> {
    fn validate_recovery_manifest(&self) -> AndromedaResult<()> {
        self.manifest.validate()
    }

    fn mounted_snapshot_id(&self) -> u64 {
        self.manifest.snapshot_id
    }

    fn required_wal_start_lsn(&self) -> Lsn {
        self.manifest.required_wal_start_lsn
    }

    fn validate_recovery_storage_formats(&self, startup_mode: StartupMode) -> AndromedaResult<()> {
        let storage_format_manifest = self.manifest.storage_format_manifest()?;
        crate::recovery::PreRedoStorageFormatGate::validate_replay_from_manifest(
            startup_mode,
            crate::recovery::RECOVERY_REQUIRED_STORAGE_FORMATS,
            &storage_format_manifest,
        )
    }
}
