//! Storage-facing durability fence checks for manifest and page publication.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use andromeda_manifest::{validate_manifest_atomic_switch, validate_recovery_floor};
use andromeda_storage_page::validate_wal_durability_before_page_flush as page_validate_wal_durability_before_page_flush;
use andromeda_wal::Lsn;

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

pub fn validate_wal_durability_before_page_flush(
    page_lsn: Lsn,
    wal_durable_lsn: Lsn,
) -> AndromedaResult<()> {
    page_validate_wal_durability_before_page_flush(page_lsn, wal_durable_lsn)
        .map_err(|error| storage_error(error.to_string()))
}
