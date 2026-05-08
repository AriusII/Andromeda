use crate::{Lsn, WalRecord};
use andromeda_core::AndromedaResult;

use super::{
    errors::storage_error, lsn::validate_lsn_continuity, segment::validate_segment_boundary,
    size::validate_record_size,
};

/// Comprehensive bounds check for a single record before append.
///
/// Validates record structure first, then hard size limits.
pub fn validate_wal_record_bounds(record: &WalRecord) -> AndromedaResult<()> {
    record.validate()?;
    validate_record_size(record)
}

/// Comprehensive bounds check for a batch of records before flush.
///
/// Validates:
/// - all records pass individual size checks
/// - cumulative batch size does not exceed the segment boundary
/// - LSN continuity is maintained across the batch
pub fn validate_wal_batch_bounds(
    base_previous_lsn: Option<Lsn>,
    records: &[WalRecord],
) -> AndromedaResult<(u64, Lsn)> {
    if records.is_empty() {
        return Err(storage_error("cannot validate empty record batch"));
    }

    let total_size = validate_segment_boundary(records)?;
    let last_lsn = validate_lsn_continuity(base_previous_lsn, records)?;

    Ok((total_size, last_lsn))
}
