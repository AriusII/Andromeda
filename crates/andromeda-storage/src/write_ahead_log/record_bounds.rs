//! WAL record cardinality and size bounds enforcement.
//!
//! This module defines and enforces hard limits on WAL records to prevent:
//! - unbounded memory allocation (`WAL_RECORD_SIZE_LIMIT`)
//! - excessive transaction batch sizes (`WAL_BATCH_ROW_LIMIT`)
//! - segment overflow (explicit boundary checks)
//! - LSN continuity violations (monotonic growth, no gaps)
//!
//! These invariants are enforced at append time by the WAL manager.

use crate::{Lsn, WalRecord, WalRecordKind};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Maximum size for a single WAL record payload (1 MB).
///
/// This prevents unbounded allocation during record encoding/decoding.
/// Records exceeding this limit are rejected at append time.
pub const WAL_RECORD_SIZE_LIMIT: u64 = 1024 * 1024; // 1 MB

/// Maximum number of rows per transaction batch.
///
/// This enforces a reasonable transaction size. Transactions with more
/// than this many row operations are rejected to prevent:
/// - excessive memory pressure during transaction processing
/// - unbounded growth of update batches
/// - recovery replay slowdowns
pub const WAL_BATCH_ROW_LIMIT: usize = 256;

/// Segment boundary size (4 MB default segment).
///
/// When flushing WAL records to a segment, cumulative record size must not
/// exceed this boundary. Set conservatively to allow for metadata overhead.
pub const WAL_SEGMENT_BOUNDARY: u64 = 4 * 1024 * 1024; // 4 MB

/// Maximum WAL header size (includes frame header and metadata).
/// Records are capped at 72 bytes base header + payload up to the limit.
pub const WAL_RECORD_HEADER_OVERHEAD: u64 = 72;

/// Validate record size does not exceed limit.
///
/// Returns an error if:
/// - Record header payload_length field exceeds `WAL_RECORD_SIZE_LIMIT`
/// - Actual payload buffer size exceeds the limit
///
/// # Arguments
///
/// * `record` - WAL record to validate
///
/// # Returns
///
/// `Ok(())` if record size is within bounds, or `AndromedaError` otherwise.
pub fn validate_record_size(record: &WalRecord) -> AndromedaResult<()> {
    let payload_len = record.header.payload_length;
    if payload_len > WAL_RECORD_SIZE_LIMIT {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "WAL record payload {} bytes exceeds limit {} bytes",
                payload_len, WAL_RECORD_SIZE_LIMIT
            ),
        ));
    }

    let actual_len = record.payload.len() as u64;
    if actual_len > WAL_RECORD_SIZE_LIMIT {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "WAL record actual size {} bytes exceeds limit {} bytes",
                actual_len, WAL_RECORD_SIZE_LIMIT
            ),
        ));
    }

    Ok(())
}

/// Validate that all records in a collection fit within segment boundary.
///
/// Given a list of records, computes total encoded size including overhead
/// and ensures it does not exceed `WAL_SEGMENT_BOUNDARY`.
///
/// # Arguments
///
/// * `records` - Slice of WAL records to validate
///
/// # Returns
///
/// `Ok(total_bytes)` with total encoded size, or `AndromedaError` if overflow.
pub fn validate_segment_boundary(records: &[WalRecord]) -> AndromedaResult<u64> {
    let mut total: u64 = 0;

    for record in records {
        validate_record_size(record)?;

        let record_total = WAL_RECORD_HEADER_OVERHEAD
            .checked_add(record.header.payload_length)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "WAL record total size would overflow u64",
                )
            })?;

        total = total.checked_add(record_total).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                "cumulative WAL record size would overflow u64",
            )
        })?;
    }

    if total > WAL_SEGMENT_BOUNDARY {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "cumulative WAL records {} bytes exceed segment boundary {} bytes",
                total, WAL_SEGMENT_BOUNDARY
            ),
        ));
    }

    Ok(total)
}

/// Validate LSN monotonicity and continuity in a record sequence.
///
/// Checks that:
/// - LSNs increase strictly monotonically (no duplicates)
/// - `previous_lsn` chains correctly link consecutive records
/// - Initial `base_previous_lsn` chains to first record's LSN
///
/// # Arguments
///
/// * `base_previous_lsn` - The LSN prior to the first record in sequence
/// * `records` - Slice of records to validate for continuity
///
/// # Returns
///
/// `Ok(last_lsn)` with the final record's LSN, or `AndromedaError` on discontinuity.
pub fn validate_lsn_continuity(
    base_previous_lsn: Option<Lsn>,
    records: &[WalRecord],
) -> AndromedaResult<Lsn> {
    if records.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "cannot validate LSN continuity for empty record list",
        ));
    }

    let first = records.first().ok_or_else(|| {
        AndromedaError::new(AndromedaErrorKind::Storage, "record list must not be empty")
    })?;

    // Check that first record's previous_lsn chains to base_previous_lsn.
    match base_previous_lsn {
        Some(base_prev) => {
            if first.header.previous_lsn != Some(base_prev) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "first record previous_lsn mismatch: expected {:?}, got {:?}",
                        Some(base_prev),
                        first.header.previous_lsn
                    ),
                ));
            }
        }
        None => {
            if first.header.previous_lsn.is_some() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "first record should have no previous_lsn, got {:?}",
                        first.header.previous_lsn
                    ),
                ));
            }
        }
    }

    let mut expected_previous = base_previous_lsn;
    for (index, record) in records.iter().enumerate() {
        // LSN must match expected sequence.
        let expected_lsn = match expected_previous {
            Some(prev) => prev.try_next()?,
            None => Lsn::new(1), // First LSN in WAL is 1 if no base.
        };

        if record.header.lsn != expected_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "record {} LSN mismatch: expected {}, got {}",
                    index,
                    expected_lsn.get(),
                    record.header.lsn.get()
                ),
            ));
        }

        // previous_lsn must link correctly.
        if record.header.previous_lsn != expected_previous {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "record {} previous_lsn mismatch: expected {:?}, got {:?}",
                    index, expected_previous, record.header.previous_lsn
                ),
            ));
        }

        expected_previous = Some(record.header.lsn);
    }

    let last = records.last().ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Storage,
            "no records to extract final LSN",
        )
    })?;

    Ok(last.header.lsn)
}

/// Validate transaction batch cardinality for a transaction's records.
///
/// Counts all row-operation records (Insert/Update/Delete) in a transaction's
/// portion of the WAL and rejects if exceeding `WAL_BATCH_ROW_LIMIT`.
///
/// # Arguments
///
/// * `records` - All records in a transaction
///
/// # Returns
///
/// `Ok(row_count)` with the number of row operations, or `AndromedaError` if limit exceeded.
pub fn validate_transaction_batch_cardinality(records: &[WalRecord]) -> AndromedaResult<usize> {
    let mut row_count = 0usize;

    for record in records {
        match record.header.kind {
            WalRecordKind::RowInsert | WalRecordKind::RowUpdate | WalRecordKind::RowDelete => {
                row_count = row_count.saturating_add(1);
                if row_count > WAL_BATCH_ROW_LIMIT {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "transaction batch size {} exceeds limit {}",
                            row_count, WAL_BATCH_ROW_LIMIT
                        ),
                    ));
                }
            }
            _ => {} // Non-row operations don't count toward batch limit.
        }
    }

    Ok(row_count)
}

/// Comprehensive bounds check for a single record before append.
///
/// Validates:
/// - Record size is within limit
/// - Record passes structural validation (checksum, LSN format, etc.)
/// - No individual record violates hardened bounds
///
/// This is called by `InMemoryWal::append()`.
///
/// # Arguments
///
/// * `record` - Record to validate before appending
///
/// # Returns
///
/// `Ok(())` if record is acceptable, or `AndromedaError` otherwise.
pub fn validate_wal_record_bounds(record: &WalRecord) -> AndromedaResult<()> {
    // Record structure validation (checksum, LSN, transaction_id requirements).
    record.validate()?;

    // Size bounds validation.
    validate_record_size(record)?;

    Ok(())
}

/// Comprehensive bounds check for a batch of records before flush.
///
/// Validates:
/// - All records pass individual size checks
/// - Cumulative batch does not exceed segment boundary
/// - LSN continuity is maintained across batch
///
/// This is called before flushing records to persistent storage.
///
/// # Arguments
///
/// * `base_previous_lsn` - The LSN before this batch in the WAL sequence
/// * `records` - Records to validate for batch flush
///
/// # Returns
///
/// `Ok((total_size, last_lsn))` on success, or `AndromedaError` otherwise.
pub fn validate_wal_batch_bounds(
    base_previous_lsn: Option<Lsn>,
    records: &[WalRecord],
) -> AndromedaResult<(u64, Lsn)> {
    if records.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "cannot validate empty record batch",
        ));
    }

    // Check segment boundary.
    let total_size = validate_segment_boundary(records)?;

    // Check LSN continuity.
    let last_lsn = validate_lsn_continuity(base_previous_lsn, records)?;

    Ok((total_size, last_lsn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::TransactionId;

    fn row_record(lsn: u64, previous: Option<u64>) -> WalRecord {
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(lsn),
            previous.map(Lsn::new),
            Some(TransactionId::new(1)),
            vec![1u8; 100],
        )
        .unwrap()
    }

    fn sized_record(lsn: u64, previous: Option<u64>, payload_size: usize) -> WalRecord {
        WalRecord::from_parts(
            WalRecordKind::RowInsert,
            Lsn::new(lsn),
            previous.map(Lsn::new),
            Some(TransactionId::new(1)),
            vec![0u8; payload_size],
        )
        .unwrap()
    }

    #[test]
    fn record_size_within_limit_passes() {
        let record = sized_record(1, None, 1000);
        assert!(validate_record_size(&record).is_ok());
    }

    #[test]
    fn record_size_at_limit_passes() {
        let record = sized_record(1, None, WAL_RECORD_SIZE_LIMIT as usize);
        assert!(validate_record_size(&record).is_ok());
    }

    #[test]
    fn record_size_exceeds_limit_fails() {
        let mut record = sized_record(1, None, (WAL_RECORD_SIZE_LIMIT as usize) + 1);
        record.header.payload_length = WAL_RECORD_SIZE_LIMIT + 1;
        assert!(validate_record_size(&record).is_err());
    }

    #[test]
    fn single_record_segment_boundary_passes() {
        let record = sized_record(1, None, 1000);
        let result = validate_segment_boundary(&[record]);
        assert!(result.is_ok());
        let total = result.unwrap();
        assert!(total <= WAL_SEGMENT_BOUNDARY);
    }

    #[test]
    fn multiple_records_within_boundary_pass() {
        let records = vec![
            row_record(1, None),
            row_record(2, Some(1)),
            row_record(3, Some(2)),
        ];
        let result = validate_segment_boundary(&records);
        assert!(result.is_ok());
    }

    #[test]
    fn segment_boundary_exceeded_fails() {
        let record_size = 2 * 1024 * 1024; // 2 MB per record
        let records = vec![
            sized_record(1, None, record_size),
            sized_record(2, Some(1), record_size),
            sized_record(3, Some(2), record_size),
        ];
        assert!(validate_segment_boundary(&records).is_err());
    }

    #[test]
    fn lsn_continuity_single_record_no_base() {
        let record = row_record(1, None);
        let result = validate_lsn_continuity(None, &[record]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Lsn::new(1));
    }

    #[test]
    fn lsn_continuity_multiple_records_no_base() {
        let records = vec![
            row_record(1, None),
            row_record(2, Some(1)),
            row_record(3, Some(2)),
        ];
        let result = validate_lsn_continuity(None, &records);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Lsn::new(3));
    }

    #[test]
    fn lsn_continuity_with_base_previous() {
        let records = vec![row_record(5, Some(4)), row_record(6, Some(5))];
        let result = validate_lsn_continuity(Some(Lsn::new(4)), &records);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Lsn::new(6));
    }

    #[test]
    fn lsn_continuity_gap_fails() {
        let records = vec![
            row_record(1, None),
            row_record(3, Some(1)), // Gap: should be 2
        ];
        assert!(validate_lsn_continuity(None, &records).is_err());
    }

    #[test]
    fn lsn_continuity_duplicate_fails() {
        let records = vec![
            row_record(1, None),
            row_record(1, Some(0)), // Duplicate LSN
        ];
        assert!(validate_lsn_continuity(None, &records).is_err());
    }

    #[test]
    fn lsn_continuity_previous_mismatch_fails() {
        let records = vec![
            row_record(1, None),
            row_record(2, None), // Should be Some(1)
        ];
        assert!(validate_lsn_continuity(None, &records).is_err());
    }

    #[test]
    fn lsn_continuity_base_mismatch_fails() {
        let records = vec![row_record(5, Some(3))]; // Should chain to Some(4)
        assert!(validate_lsn_continuity(Some(Lsn::new(4)), &records).is_err());
    }

    #[test]
    fn transaction_batch_cardinality_zero_rows() {
        let records = vec![
            row_record(1, None),
            row_record(2, Some(1)),
            row_record(3, Some(2)),
        ];
        let result = validate_transaction_batch_cardinality(&records);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn transaction_batch_cardinality_within_limit() {
        let mut records = vec![row_record(1, None)];
        for i in 2..=100 {
            records.push(row_record(i as u64, Some((i - 1) as u64)));
        }
        let result = validate_transaction_batch_cardinality(&records);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn transaction_batch_cardinality_at_limit() {
        let mut records = vec![row_record(1, None)];
        for i in 2..=256 {
            records.push(row_record(i as u64, Some((i - 1) as u64)));
        }
        let result = validate_transaction_batch_cardinality(&records);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 256);
    }

    #[test]
    fn transaction_batch_cardinality_exceeds_limit_fails() {
        let mut records = vec![row_record(1, None)];
        for i in 2..=257 {
            records.push(row_record(i as u64, Some((i - 1) as u64)));
        }
        assert!(validate_transaction_batch_cardinality(&records).is_err());
    }

    #[test]
    fn comprehensive_record_bounds_valid() {
        let record = row_record(1, None);
        assert!(validate_wal_record_bounds(&record).is_ok());
    }

    #[test]
    fn comprehensive_batch_bounds_valid() {
        let records = vec![
            row_record(1, None),
            row_record(2, Some(1)),
            row_record(3, Some(2)),
        ];
        let result = validate_wal_batch_bounds(None, &records);
        assert!(result.is_ok());
        let (total, last_lsn) = result.unwrap();
        assert!(total <= WAL_SEGMENT_BOUNDARY);
        assert_eq!(last_lsn, Lsn::new(3));
    }

    #[test]
    fn comprehensive_batch_bounds_empty_fails() {
        assert!(validate_wal_batch_bounds(None, &[]).is_err());
    }

    #[test]
    fn comprehensive_batch_bounds_segment_overflow_fails() {
        let record_size = 2 * 1024 * 1024;
        let records = vec![
            sized_record(1, None, record_size),
            sized_record(2, Some(1), record_size),
            sized_record(3, Some(2), record_size),
        ];
        assert!(validate_wal_batch_bounds(None, &records).is_err());
    }
}
