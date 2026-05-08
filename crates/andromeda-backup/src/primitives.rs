use std::fmt::Debug;

use super::error::{BackupResult, backup_error};
use andromeda_core::CatalogVersion as CoreCatalogVersion;
use andromeda_observe::TraceId as ObserveTraceId;
use andromeda_wal::Lsn as WalLsn;

pub const WAL_FORMAT_VERSION: u16 = 1;

pub trait BackupLsn: Copy + Ord + Eq + Debug {
    fn new(value: u64) -> Self;

    fn get(self) -> u64;

    fn is_zero(self) -> bool {
        self.get() == 0
    }

    fn try_next(self) -> BackupResult<Self> {
        self.get()
            .checked_add(1)
            .map(Self::new)
            .ok_or_else(|| backup_error("LSN advancement would overflow u64"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Lsn(u64);

impl Lsn {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u64::MAX);

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

impl BackupLsn for Lsn {
    fn new(value: u64) -> Self {
        Self::new(value)
    }

    fn get(self) -> u64 {
        self.get()
    }
}

impl BackupLsn for WalLsn {
    fn new(value: u64) -> Self {
        Self::new(value)
    }

    fn get(self) -> u64 {
        self.get()
    }
}

pub trait BackupCatalogVersion: Copy + Eq + Debug {
    fn get(self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CatalogVersion(u64);

impl CatalogVersion {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl BackupCatalogVersion for CatalogVersion {
    fn get(self) -> u64 {
        self.get()
    }
}

impl BackupCatalogVersion for CoreCatalogVersion {
    fn get(self) -> u64 {
        self.get()
    }
}

pub trait BackupTraceId: Copy + Eq + Debug {
    fn is_zero(self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TraceId(u128);

impl TraceId {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u128 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl BackupTraceId for TraceId {
    fn is_zero(self) -> bool {
        self.is_zero()
    }
}

impl BackupTraceId for ObserveTraceId {
    fn is_zero(self) -> bool {
        self.is_zero()
    }
}
