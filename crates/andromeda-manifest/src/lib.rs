#![forbid(unsafe_code)]
#![doc = r#"
Boundary crate for Andromeda manifests and root pointers.

This crate owns manifest durability boundary contracts that are independent of
storage page and segment implementation. Storage keeps the current manifest
facade during the migration and delegates boundary validation here.

C5 invariants:

- Manifest switches must prove durable WAL coverage for checkpoint and recovery-floor evidence.
- Manifest bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::Lsn;

/// Durable recovery root fields carried by an accepted database manifest.
///
/// This is a domain boundary, not a disk codec. Persistent manifest bytes still
/// require explicit little-endian encoding, version fields, and checksums before
/// promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestDurabilityBoundary {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub previous_manifest_hash: [u8; 32],
    pub manifest_crc: u32,
}

impl ManifestDurabilityBoundary {
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
            return Err(manifest_error("manifest identity fields must not be zero"));
        }

        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(manifest_error(
                "manifest required WAL start LSN must not precede checkpoint LSN",
            ));
        }

        if self.manifest_crc == 0 {
            return Err(manifest_error("manifest CRC must not be zero"));
        }

        Ok(())
    }
}

fn manifest_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary() -> ManifestDurabilityBoundary {
        ManifestDurabilityBoundary {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(10),
            required_wal_start_lsn: Lsn::new(10),
            previous_manifest_hash: [4; 32],
            manifest_crc: 5,
        }
    }

    #[test]
    fn manifest_boundary_validates_identity_crc_and_recovery_floor() {
        assert!(boundary().validate().is_ok());

        let mut invalid = boundary();
        invalid.manifest_crc = 0;
        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let mut invalid = boundary();
        invalid.required_wal_start_lsn = Lsn::new(9);
        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
