//! V0 backup metadata and Point-In-Time Recovery target validation.
//!
//! Modules:
//! - `physical_plan`: BackupPhysicalPlan — deterministic, pure planning
//! - `scheduler`: BackupIOScheduler — I/O-budgeted task scheduling
//! - `checkpoint_manager`: BackupCheckpointManager — crash-safe resumption
//! - `wal_archive_integration`: WAL archive integration with manifest finalization

#[allow(dead_code, unused_imports)]
#[path = "../../andromeda-backup/src/lib.rs"]
mod backup_owner;
#[allow(dead_code, unused_imports)]
#[path = "../../andromeda-restore/src/lib.rs"]
mod restore_owner;

mod artifact_store;
mod artifacts;
mod checkpoint_manager;
mod execution_plan;
mod helpers;
mod physical_plan;
mod plan;
mod scheduler;
mod types;
mod wal_archive_integration;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

impl backup_owner::BackupLsn for crate::Lsn {
    fn new(value: u64) -> Self {
        Self::new(value)
    }

    fn get(self) -> u64 {
        self.get()
    }
}

impl backup_owner::BackupCatalogVersion for andromeda_core::CatalogVersion {
    fn get(self) -> u64 {
        self.get()
    }
}

impl backup_owner::BackupTraceId for andromeda_observe::TraceId {
    fn is_zero(self) -> bool {
        self.is_zero()
    }
}

impl restore_owner::RestoreLsn for crate::Lsn {
    fn new(value: u64) -> Self {
        Self::new(value)
    }

    fn get(self) -> u64 {
        self.get()
    }
}

impl restore_owner::PitrBackupManifest<crate::Lsn, backup_owner::BackupId>
    for backup_owner::BackupManifest<crate::Lsn>
{
    fn backup_manifest_valid(&self) -> bool {
        self.validate().is_ok()
    }

    fn backup_id(&self) -> backup_owner::BackupId {
        self.backup_id
    }

    fn database_id(&self) -> u64 {
        self.database_id
    }

    fn snapshot_id(&self) -> u64 {
        self.snapshot.snapshot_id
    }

    fn base_checkpoint_lsn(&self) -> crate::Lsn {
        self.snapshot.base_checkpoint_lsn
    }

    fn required_wal_start_lsn(&self) -> crate::Lsn {
        self.snapshot.required_wal_start_lsn
    }

    fn wal_archive_start(&self) -> crate::Lsn {
        self.wal_archive.start
    }

    fn wal_archive_end_inclusive(&self) -> crate::Lsn {
        self.wal_archive.end_inclusive
    }
}

impl From<backup_owner::BackupValidationError> for AndromedaError {
    fn from(error: backup_owner::BackupValidationError) -> Self {
        AndromedaError::new(AndromedaErrorKind::Storage, error.message())
    }
}

impl From<restore_owner::RestoreValidationError> for AndromedaError {
    fn from(error: restore_owner::RestoreValidationError) -> Self {
        AndromedaError::new(AndromedaErrorKind::Storage, error.message())
    }
}

// Re-export from parent crate for convenience
pub use crate::Lsn;

pub use artifact_store::*;
pub use artifacts::*;
pub use checkpoint_manager::*;
pub use execution_plan::*;
pub use physical_plan::*;
pub use scheduler::*;
pub use types::*;
pub use wal_archive_integration::*;

pub use plan::{
    BackupManifest, PitrAuditRecord, PitrTarget, PitrTargetRejection, PitrValidationAccepted,
    validate_pitr_target, validate_pitr_target_with_audit,
};
