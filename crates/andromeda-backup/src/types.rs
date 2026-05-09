use super::{
    error::{BackupResult, backup_error},
    primitives::BackupLsn,
};

pub const BACKUP_PHYSICAL_PLAN_VERSION_V0: u16 = 1;
pub const BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0: u16 = 1;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalArchiveRange<L = super::Lsn> {
    pub start: L,
    pub end_inclusive: L,
}

impl<L> WalArchiveRange<L> {
    pub const fn new(start: L, end_inclusive: L) -> Self {
        Self {
            start,
            end_inclusive,
        }
    }
}

impl<L: BackupLsn> WalArchiveRange<L> {
    pub fn validate(self) -> BackupResult<()> {
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

    pub fn contains(self, lsn: L) -> bool {
        lsn >= self.start && lsn <= self.end_inclusive
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdSnapshotBoundary<L = super::Lsn> {
    pub snapshot_id: u64,
    pub snapshot_descriptor_hash: [u8; 32],
    pub base_checkpoint_lsn: L,
    pub required_wal_start_lsn: L,
}

impl<L: BackupLsn> ColdSnapshotBoundary<L> {
    pub fn validate(&self) -> BackupResult<()> {
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
