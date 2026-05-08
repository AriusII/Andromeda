use crate::WalRecord;
use andromeda_core::AndromedaResult;

use super::{constants::WAL_SEGMENT_BOUNDARY, errors::storage_error, size::encoded_record_size};

/// Validate that all records in a collection fit within the segment boundary.
///
/// Returns the cumulative encoded size including conservative header overhead.
pub fn validate_segment_boundary(records: &[WalRecord]) -> AndromedaResult<u64> {
    let mut total = 0u64;

    for record in records {
        let record_total = encoded_record_size(record)?;
        total = total
            .checked_add(record_total)
            .ok_or_else(|| storage_error("cumulative WAL record size would overflow u64"))?;
    }

    if total > WAL_SEGMENT_BOUNDARY {
        return Err(storage_error(format!(
            "cumulative WAL records {} bytes exceed segment boundary {} bytes",
            total, WAL_SEGMENT_BOUNDARY
        )));
    }

    Ok(total)
}
