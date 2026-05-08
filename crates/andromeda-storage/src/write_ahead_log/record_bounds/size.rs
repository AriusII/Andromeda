use crate::WalRecord;
use andromeda_core::AndromedaResult;

use super::{
    constants::{WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_SIZE_LIMIT},
    errors::storage_error,
};

/// Validate record payload size does not exceed the hard limit.
pub fn validate_record_size(record: &WalRecord) -> AndromedaResult<()> {
    let payload_len = record.header.payload_length;
    if payload_len > WAL_RECORD_SIZE_LIMIT {
        return Err(storage_error(format!(
            "WAL record payload {} bytes exceeds limit {} bytes",
            payload_len, WAL_RECORD_SIZE_LIMIT
        )));
    }

    let actual_len = record.payload.len() as u64;
    if actual_len > WAL_RECORD_SIZE_LIMIT {
        return Err(storage_error(format!(
            "WAL record actual size {} bytes exceeds limit {} bytes",
            actual_len, WAL_RECORD_SIZE_LIMIT
        )));
    }

    Ok(())
}

pub(super) fn encoded_record_size(record: &WalRecord) -> AndromedaResult<u64> {
    validate_record_size(record)?;
    WAL_RECORD_HEADER_OVERHEAD
        .checked_add(record.header.payload_length)
        .ok_or_else(|| storage_error("WAL record total size would overflow u64"))
}
