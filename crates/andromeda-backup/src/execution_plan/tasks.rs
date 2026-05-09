use std::fmt::Debug;

use andromeda_segment::{ExtentDescriptor, ExtentState};
use andromeda_wal::WalSegmentDescriptor;

use crate::error::{BackupResult, backup_error, map_core_validation};

/// Storage tier classification used by backup execution planning.
pub trait BackupSourceTier: Copy + Eq + Debug {
    fn is_durable_backup_source(self) -> bool;
}

/// Standalone backup-owned tier enum for callers outside `andromeda-storage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupStorageTier {
    Ram,
    HotStore,
    ColdStore,
}

impl BackupSourceTier for BackupStorageTier {
    fn is_durable_backup_source(self) -> bool {
        matches!(self, Self::HotStore | Self::ColdStore)
    }
}

/// Single extent copy task for the physical backup.
///
/// Extent identity + storage tier determines the copy source.
/// ColdStore extents are preferred; HotStore extents may be present during backup
/// if not yet migrated to cold storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtentCopyTask<S = BackupStorageTier> {
    /// Canonical extent descriptor (contains page count, LSN bounds, segment reference)
    pub extent_descriptor: ExtentDescriptor,
    /// Source storage tier (HotStore NVMe or ColdStore HDD)
    pub source_tier: S,
    /// Byte count for this extent (page_count * page_size, approximately)
    pub byte_count: u64,
}

impl<S: BackupSourceTier> ExtentCopyTask<S> {
    pub fn validate(&self) -> BackupResult<()> {
        map_core_validation(self.extent_descriptor.validate())?;

        // Backup must copy from durable storage only.
        if matches!(self.extent_descriptor.state, ExtentState::AllocatingHot) {
            return Err(backup_error(
                "extent copy task must not reference hot/mutable extent",
            ));
        }

        if self.byte_count == 0 {
            return Err(backup_error("extent copy task byte count must not be zero"));
        }

        // ColdStore extents are immutable; HotStore may have sealed extents that are durable.
        if !self.source_tier.is_durable_backup_source() {
            return Err(backup_error(
                "extent copy task source tier must be HotStore or ColdStore, not RAM",
            ));
        }

        Ok(())
    }
}

/// Single WAL segment copy task in the physical backup.
///
/// WAL segments must form a contiguous chain from `wal_start_lsn` to `wal_end_lsn`.
/// Each segment's LSN bounds and `base_previous_lsn` are validated to ensure no gaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalSegmentCopyTask {
    /// Canonical WAL segment descriptor (first_lsn, last_lsn, base_previous_lsn, record_count)
    pub segment_descriptor: WalSegmentDescriptor,
    /// Byte count for this segment's durable records
    pub byte_count: u64,
    /// Position in the sequential WAL archive (0 = first segment)
    pub sequence_index: usize,
}

impl WalSegmentCopyTask {
    pub fn validate(&self) -> BackupResult<()> {
        map_core_validation(self.segment_descriptor.validate())?;

        if self.byte_count == 0 {
            return Err(backup_error(
                "WAL segment copy task byte count must not be zero",
            ));
        }

        Ok(())
    }
}
