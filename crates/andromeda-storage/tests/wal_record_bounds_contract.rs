//! Contract tests for WAL record cardinality bounds enforcement.
//!
//! This test suite validates:
//! - Record size limits (1 MB maximum)
//! - Transaction row cardinality bounds (256 rows maximum per transaction)
//! - Segment overflow rejection
//! - LSN monotonicity and continuity
//! - Integration with InMemoryWal manager
//!
//! Each test case focuses on a specific invariant and verifies that the
//! bounds enforcement correctly rejects or accepts records accordingly.

use andromeda_core::TransactionId;
use andromeda_wal::{
    InMemoryWal, Lsn, WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_SIZE_LIMIT, WAL_SEGMENT_BOUNDARY,
    WalRecord, WalRecordKind, validate_lsn_continuity, validate_record_size,
    validate_segment_boundary, validate_transaction_batch_cardinality, validate_wal_batch_bounds,
    validate_wal_record_bounds,
};

/// Helper to create a test WAL record with configurable size.
fn sized_record(lsn: u64, previous: Option<u64>, payload_size: usize) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        previous.map(Lsn::new),
        Some(TransactionId::new(1)),
        vec![0u8; payload_size],
    )
    .expect("record creation should succeed")
}

/// Helper to create a row insert record.
fn row_record(lsn: u64, previous: Option<u64>) -> WalRecord {
    sized_record(lsn, previous, 100)
}

/// Helper to create a transaction begin record.
fn tx_begin(lsn: u64, tx_id: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxBegin,
        Lsn::new(lsn),
        None,
        Some(TransactionId::new(tx_id)),
        Vec::new(),
    )
    .expect("record creation should succeed")
}

/// Helper to create a transaction commit record.
fn tx_commit(lsn: u64, previous: u64, tx_id: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::TxCommit,
        Lsn::new(lsn),
        Some(Lsn::new(previous)),
        Some(TransactionId::new(tx_id)),
        Vec::new(),
    )
    .expect("record creation should succeed")
}

#[test]
fn record_size_under_limit_is_accepted() {
    let record = sized_record(1, None, 512 * 1024); // 512 KB
    assert!(validate_wal_record_bounds(&record).is_ok());
}

#[test]
fn record_size_at_limit_is_accepted() {
    let record = sized_record(1, None, WAL_RECORD_SIZE_LIMIT as usize);
    assert!(validate_wal_record_bounds(&record).is_ok());
}

#[test]
fn record_size_exceeds_limit_is_rejected() {
    let mut record = sized_record(1, None, (WAL_RECORD_SIZE_LIMIT as usize) + 1);
    // Manually adjust header to exceed limit
    record.header.payload_length = WAL_RECORD_SIZE_LIMIT + 1;
    let result = validate_wal_record_bounds(&record);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("exceeds limit"));
}

#[test]
fn record_2mb_payload_is_rejected() {
    let record = sized_record(1, None, 2 * 1024 * 1024); // 2 MB
    let result = validate_record_size(&record);
    assert!(result.is_err());
}

#[test]
fn in_memory_wal_rejects_oversized_record() {
    let mut wal = InMemoryWal::new();
    let mut record = sized_record(1, None, (WAL_RECORD_SIZE_LIMIT as usize) + 1);
    record.header.payload_length = WAL_RECORD_SIZE_LIMIT + 1;

    let result = wal.append(record);
    assert!(result.is_err());
}

#[test]
fn batch_cardinality_under_limit_is_accepted() {
    let mut records = vec![row_record(1, None)];
    for i in 2..=100 {
        records.push(row_record(i as u64, Some((i - 1) as u64)));
    }
    let result = validate_transaction_batch_cardinality(&records);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 100);
}

#[test]
fn batch_cardinality_at_limit_is_accepted() {
    let mut records = vec![row_record(1, None)];
    for i in 2..=256 {
        records.push(row_record(i as u64, Some((i - 1) as u64)));
    }
    let result = validate_transaction_batch_cardinality(&records);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 256);
}

#[test]
fn batch_cardinality_exceeds_limit_is_rejected() {
    let mut records = vec![row_record(1, None)];
    for i in 2..=257 {
        records.push(row_record(i as u64, Some((i - 1) as u64)));
    }
    let result = validate_transaction_batch_cardinality(&records);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("exceeds limit"));
}

#[test]
fn batch_with_256_rows_and_metadata_passes() {
    let tx_id = 99u64;
    let mut records = vec![tx_begin(1, tx_id)];

    // Add 256 row operations
    for i in 2..=257 {
        records.push(row_record(i as u64, Some((i - 1) as u64)));
    }

    records.push(tx_commit(258, 257, tx_id));

    // Validate only the row operations
    let row_records: Vec<_> = records
        .iter()
        .filter(|r| {
            matches!(
                r.header.kind,
                WalRecordKind::RowInsert | WalRecordKind::RowUpdate | WalRecordKind::RowDelete
            )
        })
        .cloned()
        .collect();

    assert_eq!(row_records.len(), 256);
    assert!(validate_transaction_batch_cardinality(&row_records).is_ok());
}

#[test]
fn batch_with_257_rows_fails() {
    let mut records = vec![row_record(1, None)];
    for i in 2..=257 {
        records.push(row_record(i as u64, Some((i - 1) as u64)));
    }
    assert!(validate_transaction_batch_cardinality(&records).is_err());
}

#[test]
fn single_small_record_within_segment_boundary() {
    let record = row_record(1, None);
    let result = validate_segment_boundary(&[record]);
    assert!(result.is_ok());
}

#[test]
fn multiple_records_within_segment_boundary() {
    let mut records = vec![];
    for i in 1..=10 {
        records.push(if i == 1 {
            row_record(i as u64, None)
        } else {
            row_record(i as u64, Some((i - 1) as u64))
        });
    }
    let result = validate_segment_boundary(&records);
    assert!(result.is_ok());
}

#[test]
fn segment_boundary_exceeded_is_rejected() {
    let record_size = WAL_RECORD_SIZE_LIMIT as usize; // Per-record valid, cumulative invalid.
    let records: Vec<_> = (1..=5)
        .map(|lsn| sized_record(lsn, (lsn > 1).then_some(lsn - 1), record_size))
        .collect();
    let result = validate_segment_boundary(&records);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("exceed segment boundary")
    );
}

#[test]
fn segment_boundary_at_limit_passes() {
    let record_count = 4;
    let record_size = ((WAL_SEGMENT_BOUNDARY - (record_count * WAL_RECORD_HEADER_OVERHEAD))
        / record_count) as usize;
    let records: Vec<_> = (1..=record_count)
        .map(|lsn| sized_record(lsn, (lsn > 1).then_some(lsn - 1), record_size))
        .collect();
    let result = validate_segment_boundary(&records);
    assert!(result.is_ok());
}

#[test]
fn large_batch_just_under_segment_boundary_passes() {
    let max_per_record =
        ((WAL_SEGMENT_BOUNDARY - (10 * WAL_RECORD_HEADER_OVERHEAD) - 10) / 10) as usize;
    let mut records = vec![];
    for i in 1..=10 {
        records.push(if i == 1 {
            sized_record(i as u64, None, max_per_record)
        } else {
            sized_record(i as u64, Some((i - 1) as u64), max_per_record)
        });
    }
    let result = validate_segment_boundary(&records);
    assert!(result.is_ok());
}

#[test]
fn lsn_continuity_single_record_no_base() {
    let record = row_record(1, None);
    let result = validate_lsn_continuity(None, &[record]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Lsn::new(1));
}

#[test]
fn lsn_continuity_multiple_sequential_records() {
    let records = vec![
        row_record(1, None),
        row_record(2, Some(1)),
        row_record(3, Some(2)),
        row_record(4, Some(3)),
    ];
    let result = validate_lsn_continuity(None, &records);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Lsn::new(4));
}

#[test]
fn lsn_continuity_with_base_previous_lsn() {
    let records = vec![
        row_record(10, Some(9)),
        row_record(11, Some(10)),
        row_record(12, Some(11)),
    ];
    let result = validate_lsn_continuity(Some(Lsn::new(9)), &records);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Lsn::new(12));
}

#[test]
fn lsn_continuity_gap_is_rejected() {
    let records = vec![
        row_record(1, None),
        row_record(3, Some(1)), // Gap: should be 2
    ];
    let result = validate_lsn_continuity(None, &records);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("LSN mismatch"));
}

#[test]
fn lsn_continuity_duplicate_lsn_is_rejected() {
    let records = vec![
        row_record(1, None),
        row_record(1, Some(0)), // Duplicate
    ];
    let result = validate_lsn_continuity(None, &records);
    assert!(result.is_err());
}

#[test]
fn lsn_continuity_previous_lsn_mismatch_is_rejected() {
    let records = vec![
        row_record(1, None),
        row_record(2, None), // Should be Some(1)
    ];
    let result = validate_lsn_continuity(None, &records);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("previous_lsn mismatch")
    );
}

#[test]
fn lsn_continuity_base_chain_mismatch_is_rejected() {
    let records = vec![row_record(5, Some(3))]; // Should chain to Some(4)
    let result = validate_lsn_continuity(Some(Lsn::new(4)), &records);
    assert!(result.is_err());
}

#[test]
fn lsn_continuity_rejects_decreasing_lsn() {
    let records = vec![
        row_record(5, None),
        row_record(4, Some(3)), // Decreasing LSN
    ];
    let result = validate_lsn_continuity(None, &records);
    assert!(result.is_err());
}

#[test]
fn batch_bounds_valid_records_pass() {
    let records = vec![
        row_record(1, None),
        row_record(2, Some(1)),
        row_record(3, Some(2)),
    ];
    let result = validate_wal_batch_bounds(None, &records);
    assert!(result.is_ok());
    let (total_size, last_lsn) = result.unwrap();
    assert!(total_size <= WAL_SEGMENT_BOUNDARY);
    assert_eq!(last_lsn, Lsn::new(3));
}

#[test]
fn batch_bounds_empty_fails() {
    let result = validate_wal_batch_bounds(None, &[]);
    assert!(result.is_err());
}

#[test]
fn batch_bounds_segment_overflow_fails() {
    let record_size = 2 * 1024 * 1024;
    let records = vec![
        sized_record(1, None, record_size),
        sized_record(2, Some(1), record_size),
        sized_record(3, Some(2), record_size),
    ];
    let result = validate_wal_batch_bounds(None, &records);
    assert!(result.is_err());
}

#[test]
fn batch_bounds_lsn_gap_fails() {
    let records = vec![
        row_record(1, None),
        row_record(3, Some(1)), // Gap
    ];
    let result = validate_wal_batch_bounds(None, &records);
    assert!(result.is_err());
}

#[test]
fn batch_bounds_with_base_previous_lsn() {
    let records = vec![row_record(5, Some(4)), row_record(6, Some(5))];
    let result = validate_wal_batch_bounds(Some(Lsn::new(4)), &records);
    assert!(result.is_ok());
    let (_, last_lsn) = result.unwrap();
    assert_eq!(last_lsn, Lsn::new(6));
}

// Integration Tests with InMemoryWal Manager

#[test]
fn in_memory_wal_appends_valid_record() {
    let mut wal = InMemoryWal::new();
    let record = tx_begin(1, 1);

    let result = wal.append(record);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Lsn::new(1));
    assert_eq!(wal.len(), 1);
}

#[test]
fn in_memory_wal_enforces_monotonic_lsn() {
    let mut wal = InMemoryWal::new();
    let record1 = tx_begin(1, 1);
    let record2_bad = row_record(1, Some(0)); // Duplicate LSN

    assert!(wal.append(record1).is_ok());
    let result = wal.append(record2_bad);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("LSN must equal"));
}

#[test]
fn in_memory_wal_appends_sequence_of_records() {
    let mut wal = InMemoryWal::new();

    for i in 1..=10 {
        let record = if i == 1 {
            row_record(i as u64, None)
        } else {
            row_record(i as u64, Some((i - 1) as u64))
        };

        let result = wal.append(record);
        assert!(result.is_ok(), "append record {} failed", i);
        assert_eq!(result.unwrap(), Lsn::new(i as u64));
    }

    assert_eq!(wal.len(), 10);
}

#[test]
fn in_memory_wal_rejects_oversized_record_on_append() {
    let mut wal = InMemoryWal::new();
    let mut record = sized_record(1, None, (WAL_RECORD_SIZE_LIMIT as usize) + 1);
    record.header.payload_length = WAL_RECORD_SIZE_LIMIT + 1;

    let result = wal.append(record);
    assert!(result.is_err());
    assert_eq!(wal.len(), 0);
}

#[test]
fn in_memory_wal_preserves_lsn_on_failed_append() {
    let mut wal = InMemoryWal::new();

    // Successful first append
    let record1 = tx_begin(1, 1);
    assert!(wal.append(record1).is_ok());

    // Failed append (bad LSN)
    let record2_bad = row_record(1, Some(0));
    assert!(wal.append(record2_bad).is_err());

    // Next successful append should still use LSN 2
    let record2_ok = row_record(2, Some(1));
    let result = wal.append(record2_ok);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Lsn::new(2));
}

#[test]
fn exactly_one_megabyte_record() {
    let record = sized_record(1, None, 1024 * 1024);
    assert!(validate_record_size(&record).is_ok());
}

#[test]
fn exactly_one_megabyte_plus_one_fails() {
    let mut record = sized_record(1, None, 1024 * 1024 + 1);
    record.header.payload_length = 1024 * 1024 + 1;
    assert!(validate_record_size(&record).is_err());
}

#[test]
fn test_256_rows_exactly_at_boundary() {
    let mut records = vec![];
    for i in 1..=256 {
        records.push(if i == 1 {
            row_record(i as u64, None)
        } else {
            row_record(i as u64, Some((i - 1) as u64))
        });
    }
    assert_eq!(
        validate_transaction_batch_cardinality(&records).unwrap(),
        256
    );
}

#[test]
fn rows_257_fail() {
    let mut records = vec![];
    for i in 1..=257 {
        records.push(if i == 1 {
            row_record(i as u64, None)
        } else {
            row_record(i as u64, Some((i - 1) as u64))
        });
    }
    assert!(validate_transaction_batch_cardinality(&records).is_err());
}

#[test]
fn transaction_metadata_records_do_not_count_toward_row_limit() {
    let records = vec![tx_begin(1, 1), row_record(2, Some(1)), tx_commit(3, 2, 1)];
    let row_count = validate_transaction_batch_cardinality(&records).unwrap();
    assert_eq!(row_count, 1); // Only the RowInsert counts
}

#[test]
fn non_row_operations_ignored_in_batch_cardinality() {
    let records = vec![
        tx_begin(1, 1),
        tx_begin(2, 1),
        tx_begin(3, 1),
        row_record(4, Some(3)),
        tx_commit(5, 4, 1),
    ];
    let row_count = validate_transaction_batch_cardinality(&records).unwrap();
    assert_eq!(row_count, 1);
}

#[test]
fn lsn_max_should_reject_on_next() {
    let records = vec![row_record(u64::MAX, None)];
    let result = validate_lsn_continuity(None, &records);
    // Without a base LSN, the first record must start at LSN 1.
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("LSN mismatch"));
}
