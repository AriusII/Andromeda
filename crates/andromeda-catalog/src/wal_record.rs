//! Compatibility methods for catalog-local mutation records.

use andromeda_error::{AndromedaError, AndromedaResult};

use crate::CatalogMutationRecord;

impl CatalogMutationRecord {
    /// Encode this catalog mutation record as a durable storage-WAL payload.
    ///
    /// The caller must wrap the resulting bytes in a storage WAL record whose
    /// kind tag matches [`crate::CatalogMutationRecordKind::storage_wal_kind_tag`].
    pub fn encode_durable_payload(&self) -> AndromedaResult<Vec<u8>> {
        let recovery_record = andromeda_catalog_recovery::CatalogMutationRecord::from(self.clone());
        andromeda_catalog_recovery::encode_catalog_durable_payload(&recovery_record)
    }

    /// Decode a durable catalog mutation payload.
    ///
    /// This validates the payload header, body checksum, kind/body agreement,
    /// and catalog-local structural invariants. It does not publish the decoded
    /// mutation or replay it into a snapshot.
    pub fn decode_durable_payload(payload: &[u8]) -> AndromedaResult<Self> {
        andromeda_catalog_recovery::decode_catalog_durable_payload(payload)
            .map(Self::from)
            .map_err(AndromedaError::from)
    }

    pub fn validate_for_durable_payload(&self) -> AndromedaResult<()> {
        let recovery_record = andromeda_catalog_recovery::CatalogMutationRecord::from(self.clone());
        andromeda_catalog_recovery::validate_catalog_mutation_record(&recovery_record)
    }
}
