use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseManifest {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub previous_manifest_hash: [u8; 32],
    pub manifest_crc: u32,
}

impl DatabaseManifest {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.database_id == 0 || self.manifest_version == 0 || self.snapshot_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest identity fields must not be zero",
            ));
        }

        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest required WAL start LSN must not precede checkpoint LSN",
            ));
        }

        if self.manifest_crc == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest CRC must not be zero",
            ));
        }

        Ok(())
    }
}
