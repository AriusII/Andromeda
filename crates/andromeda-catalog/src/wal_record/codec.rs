//! Binary codec for catalog mutation WAL payloads.
//!
//! This facade preserves the durable catalog mutation payload API while the
//! implementation is split into private modules for header compatibility,
//! checksum, primitive field access, body encode/decode, and validation.  The
//! persistent byte format remains versioned and little-endian.

mod checksum;
mod decode;
mod encode;
mod error;
mod fields;
mod format;
mod validation;

use andromeda_core::{AndromedaError, AndromedaResult};

use crate::{CatalogMutationRecord, CatalogWalPayloadDecodeError};

use checksum::catalog_wal_payload_checksum;
use error::catalog_error;
use fields::Decoder;

impl CatalogMutationRecord {
    /// Encode this catalog mutation record as a durable storage-WAL payload.
    ///
    /// The caller must wrap the resulting bytes in a storage WAL record whose
    /// kind tag matches [`CatalogMutationRecordKind::storage_wal_kind_tag`].
    pub fn encode_durable_payload(&self) -> AndromedaResult<Vec<u8>> {
        encode::encode_durable_payload(self)
    }

    /// Decode a durable catalog mutation payload.
    ///
    /// This validates the payload header, body checksum, kind/body agreement,
    /// and catalog-local structural invariants. It does not publish the decoded
    /// mutation or replay it into a snapshot.
    pub fn decode_durable_payload(payload: &[u8]) -> AndromedaResult<Self> {
        Self::decode_durable_payload_typed(payload).map_err(AndromedaError::from)
    }

    pub(crate) fn decode_durable_payload_typed(
        payload: &[u8],
    ) -> Result<Self, CatalogWalPayloadDecodeError> {
        decode::decode_durable_payload(payload)
    }

    pub fn validate_for_durable_payload(&self) -> AndromedaResult<()> {
        validation::validate_record(self)
    }
}
