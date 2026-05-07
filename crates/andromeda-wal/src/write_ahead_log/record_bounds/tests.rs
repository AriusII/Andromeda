use super::*;
use crate::{Lsn, WalRecord, WalRecordKind};
use andromeda_core::TransactionId;

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

fn row_record(lsn: u64, previous: Option<u64>) -> WalRecord {
    sized_record(lsn, previous, 100)
}

fn tx_begin(lsn: u64, previous: Option<u64>) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxBegin,
        Lsn::new(lsn),
        previous.map(Lsn::new),
        Some(TransactionId::new(1)),
        Vec::new(),
    )
    .unwrap()
}

fn tx_commit(lsn: u64, previous: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxCommit,
        Lsn::new(lsn),
        Some(Lsn::new(previous)),
        Some(TransactionId::new(1)),
        Vec::new(),
    )
    .unwrap()
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
fn segment_boundary_at_limit_passes() {
    let record_count = 4;
    let record_size = ((WAL_SEGMENT_BOUNDARY - (record_count * WAL_RECORD_HEADER_OVERHEAD))
        / record_count) as usize;
    let records: Vec<_> = (1..=record_count)
        .map(|lsn| sized_record(lsn, (lsn > 1).then_some(lsn - 1), record_size))
        .collect();

    assert!(validate_segment_boundary(&records).is_ok());
}

#[test]
fn segment_boundary_exceeded_fails() {
    let record_size = WAL_RECORD_SIZE_LIMIT as usize;
    let records: Vec<_> = (1..=5)
        .map(|lsn| sized_record(lsn, (lsn > 1).then_some(lsn - 1), record_size))
        .collect();

    assert!(validate_segment_boundary(&records).is_err());
}

#[test]
fn lsn_continuity_with_base_previous() {
    let records = vec![row_record(5, Some(4)), row_record(6, Some(5))];
    let result = validate_lsn_continuity(Some(Lsn::new(4)), &records);

    assert_eq!(result.unwrap(), Lsn::new(6));
}

#[test]
fn lsn_continuity_gap_fails() {
    let records = vec![row_record(1, None), row_record(3, Some(1))];

    assert!(validate_lsn_continuity(None, &records).is_err());
}

#[test]
fn lsn_continuity_previous_mismatch_fails() {
    let records = vec![row_record(1, None), row_record(2, None)];

    assert!(validate_lsn_continuity(None, &records).is_err());
}

#[test]
fn lsn_continuity_empty_fails() {
    assert!(validate_lsn_continuity(None, &[]).is_err());
}

#[test]
fn transaction_metadata_records_do_not_count_toward_row_limit() {
    let records = vec![tx_begin(1, None), row_record(2, Some(1)), tx_commit(3, 2)];

    assert_eq!(validate_transaction_batch_cardinality(&records).unwrap(), 1);
}

#[test]
fn transaction_batch_cardinality_exceeds_limit_fails() {
    let records: Vec<_> = (1..=257)
        .map(|lsn| row_record(lsn, (lsn > 1).then_some(lsn - 1)))
        .collect();

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
