//! WAL record cardinality and size bounds facade.
//!
//! The public functions remain stable while the bounded validation logic lives
//! in this facade:
//! - hard WAL limits
//! - single-record payload and encoded-size bounds
//! - cumulative segment-boundary checks
//! - deterministic LSN continuity checks
//! - transaction row-operation limits
//! - high-level record and batch validation entry points
//!
//! These checks preserve the WAL invariants that replay is deterministic,
//! durable record order is explicit, and batch growth is bounded.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{Lsn, WalRecord, WalRecordKind};

/// Maximum size for a single WAL record payload (1 MB).
///
/// This prevents unbounded allocation during record encoding and decoding.
pub const WAL_RECORD_SIZE_LIMIT: u64 = 1024 * 1024;

/// Maximum number of row operations per transaction batch.
pub const WAL_BATCH_ROW_LIMIT: usize = 256;

/// Segment boundary size (4 MB default segment).
///
/// Cumulative encoded record size must not exceed this boundary.
pub const WAL_SEGMENT_BOUNDARY: u64 = 4 * 1024 * 1024;

/// Maximum WAL header size used for conservative encoded-size accounting.
pub const WAL_RECORD_HEADER_OVERHEAD: u64 = 72;

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

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

/// Validate transaction row-operation cardinality for a WAL record batch.
///
/// Only row operations count toward the transaction batch limit. Transaction
/// metadata records remain outside this limit.
pub fn validate_transaction_batch_cardinality(records: &[WalRecord]) -> AndromedaResult<usize> {
    let mut row_count = 0usize;

    for record in records {
        if is_transaction_row_operation(record.header.kind) {
            row_count += 1;
            if row_count > WAL_BATCH_ROW_LIMIT {
                return Err(storage_error(format!(
                    "transaction batch size {} exceeds limit {}",
                    row_count, WAL_BATCH_ROW_LIMIT
                )));
            }
        }
    }

    Ok(row_count)
}

const fn is_transaction_row_operation(kind: WalRecordKind) -> bool {
    matches!(
        kind,
        WalRecordKind::RowInsert | WalRecordKind::RowUpdate | WalRecordKind::RowDelete
    )
}

/// Validate LSN monotonicity and continuity in a record sequence.
///
/// The sequence must form an exact durable chain: each record LSN is the next
/// value after the expected previous LSN, and each `previous_lsn` links to that
/// expected value.
pub fn validate_lsn_continuity(
    base_previous_lsn: Option<Lsn>,
    records: &[WalRecord],
) -> AndromedaResult<Lsn> {
    if records.is_empty() {
        return Err(storage_error(
            "cannot validate LSN continuity for empty record list",
        ));
    }

    let mut expected_previous = base_previous_lsn;
    let mut last_lsn = records[0].header.lsn;

    for (index, record) in records.iter().enumerate() {
        let expected_lsn = match expected_previous {
            Some(previous) => previous.try_next()?,
            None => Lsn::new(1),
        };

        if record.header.lsn != expected_lsn {
            return Err(storage_error(format!(
                "record {} LSN mismatch: expected {}, got {}",
                index,
                expected_lsn.get(),
                record.header.lsn.get()
            )));
        }

        if record.header.previous_lsn != expected_previous {
            return Err(storage_error(format!(
                "record {} previous_lsn mismatch: expected {:?}, got {:?}",
                index, expected_previous, record.header.previous_lsn
            )));
        }

        last_lsn = record.header.lsn;
        expected_previous = Some(record.header.lsn);
    }

    Ok(last_lsn)
}

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

fn encoded_record_size(record: &WalRecord) -> AndromedaResult<u64> {
    validate_record_size(record)?;
    WAL_RECORD_HEADER_OVERHEAD
        .checked_add(record.header.payload_length)
        .ok_or_else(|| storage_error("WAL record total size would overflow u64"))
}

#[cfg(test)]
mod tests;
