//! Compatibility facade for backup execution planning.
//!
//! The canonical execution plan contracts live in `andromeda-backup`. Storage
//! keeps the historical type names and adapts its runtime `StorageTier`.

pub use andromeda_backup::{BackupResourceLimits, BackupSourceTier, WalSegmentCopyTask};

pub type BackupExecutionPlan = andromeda_backup::BackupExecutionPlan<crate::StorageTier>;
pub type ExtentCopyTask = andromeda_backup::ExtentCopyTask<crate::StorageTier>;

impl BackupSourceTier for crate::StorageTier {
    fn is_durable_backup_source(self) -> bool {
        matches!(self, Self::HotStore | Self::ColdStore)
    }
}
