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
    pub const fn recovery_floor_lsn(self) -> Lsn {
        self.required_wal_start_lsn
    }

    pub const fn checkpoint_lsn(self) -> Lsn {
        self.base_checkpoint_lsn
    }

    pub const fn can_start_recovery_at(self, lsn: Lsn) -> bool {
        lsn.get() >= self.required_wal_start_lsn.get()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(10),
            required_wal_start_lsn: Lsn::new(11),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        }
    }

    #[test]
    fn manifest_exposes_recovery_floor() {
        let manifest = valid_manifest();

        assert_eq!(manifest.checkpoint_lsn(), Lsn::new(10));
        assert_eq!(manifest.recovery_floor_lsn(), Lsn::new(11));
        assert!(!manifest.can_start_recovery_at(Lsn::new(10)));
        assert!(manifest.can_start_recovery_at(Lsn::new(11)));
    }

    #[test]
    fn manifest_rejects_wal_start_before_checkpoint() {
        let mut manifest = valid_manifest();
        manifest.required_wal_start_lsn = Lsn::new(9);

        assert_eq!(
            manifest.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
