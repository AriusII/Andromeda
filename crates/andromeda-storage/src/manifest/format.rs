use andromeda_core::AndromedaResult;

use crate::format_version::{FormatVersion, StorageFormatFingerprint, StorageFormatKind};

use super::{DatabaseManifest, hash::storage_format_manifest_hash, storage_error};

pub const DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS: [StorageFormatFingerprint; 5] = [
    StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
    StorageFormatFingerprint::new(StorageFormatKind::HeapPage, FormatVersion::V1_0),
    StorageFormatFingerprint::new(StorageFormatKind::BTreeKey, FormatVersion::V1_0),
    StorageFormatFingerprint::new(StorageFormatKind::BTreeNode, FormatVersion::V1_0),
    StorageFormatFingerprint::new(StorageFormatKind::WalPayload, FormatVersion::V1_0),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFormatManifest {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub fingerprints: Vec<StorageFormatFingerprint>,
    pub fingerprint_hash: [u8; 32],
}

impl StorageFormatManifest {
    pub fn new(
        database_id: u64,
        manifest_version: u64,
        snapshot_id: u64,
        fingerprints: Vec<StorageFormatFingerprint>,
    ) -> Self {
        let fingerprint_hash =
            storage_format_manifest_hash(database_id, manifest_version, snapshot_id, &fingerprints);
        Self {
            database_id,
            manifest_version,
            snapshot_id,
            fingerprints,
            fingerprint_hash,
        }
    }

    pub fn from_database_manifest(manifest: &DatabaseManifest) -> AndromedaResult<Self> {
        manifest.validate()?;
        Ok(Self::new(
            manifest.database_id,
            manifest.manifest_version,
            manifest.snapshot_id,
            DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS.to_vec(),
        ))
    }

    pub fn fingerprints(&self) -> &[StorageFormatFingerprint] {
        &self.fingerprints
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.database_id == 0 || self.manifest_version == 0 || self.snapshot_id == 0 {
            return Err(storage_error(
                "storage format manifest identity fields must not be zero",
            ));
        }
        if self.fingerprints.is_empty() {
            return Err(storage_error(
                "storage format manifest must carry durable format fingerprints",
            ));
        }
        if self.fingerprint_hash == [0; 32] {
            return Err(storage_error(
                "storage format manifest fingerprint hash must not be zero",
            ));
        }

        for (index, fingerprint) in self.fingerprints.iter().enumerate() {
            if fingerprint.version.major == 0 && fingerprint.version.minor == 0 {
                return Err(storage_error(
                    "storage format manifest fingerprint version must not be zero",
                ));
            }
            if self.fingerprints[..index]
                .iter()
                .any(|previous| previous.kind == fingerprint.kind)
            {
                return Err(storage_error(
                    "storage format manifest must not duplicate format fingerprints",
                ));
            }
        }

        let expected_hash = storage_format_manifest_hash(
            self.database_id,
            self.manifest_version,
            self.snapshot_id,
            &self.fingerprints,
        );
        if self.fingerprint_hash != expected_hash {
            return Err(storage_error(
                "storage format manifest fingerprint hash mismatch",
            ));
        }

        Ok(())
    }
}
