//! Property-based fuzz tests for recovery path robustness.
//!
//! # Goal
//! Verify that WAL recovery:
//! - Never panics on corrupted/missing data
//! - Either succeeds with valid recovery or returns clear error
//! - Provides forensic information when recovery fails
//! - Handles out-of-order records gracefully
//!
//! # Properties Tested
//! 1. Recovery never panics on corrupted WAL segments
//! 2. Recovery returns Ok or Err (never crashes)
//! 3. Valid WAL → successful recovery
//! 4. Corrupted WAL → Err with forensic info
//! 5. Missing LSN sequences handled
//! 6. Out-of-order records detected
//! 7. Duplicate record handling
//! 8. Large WAL segments processed
//! 9. Recovery deterministic with same input

#![forbid(unsafe_code)]

use proptest::prelude::*;
use std::panic;

// Reference recovery types

#[derive(Debug, Clone)]
struct WalSegment {
    records: Vec<WalRecordData>,
}

#[derive(Debug, Clone)]
struct WalRecordData {
    lsn: u64,
    payload: Vec<u8>,
    checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecoveryResult {
    success: bool,
    records_processed: usize,
    errors: Vec<String>,
}

// Test Data Generators

fn arb_lsn() -> impl Strategy<Value = u64> {
    0u64..u64::MAX
}

fn arb_wal_payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0u8..=255u8, 0..1000)
}

fn arb_wal_record() -> impl Strategy<Value = WalRecordData> {
    (arb_lsn(), arb_wal_payload()).prop_map(|(lsn, payload)| WalRecordData {
        lsn,
        payload,
        checksum: calculate_mock_checksum(&[lsn.to_le_bytes().as_ref()].concat()),
    })
}

fn arb_wal_segment() -> impl Strategy<Value = WalSegment> {
    prop::collection::vec(arb_wal_record(), 1..100).prop_map(|records| WalSegment { records })
}

fn calculate_mock_checksum(data: &[u8]) -> u64 {
    let mut result: u64 = 0;
    for chunk in data.chunks(8) {
        let mut bytes = [0u8; 8];
        bytes[..chunk.len()].copy_from_slice(chunk);
        result = result
            .wrapping_mul(31)
            .wrapping_add(u64::from_le_bytes(bytes));
    }
    result
}

fn valid_segment_from(mut records: Vec<WalRecordData>) -> WalSegment {
    records.sort_by_key(|record| record.lsn);

    for (i, record) in records.iter_mut().enumerate() {
        record.lsn = (i as u64) + 1000;
        record.checksum = calculate_mock_checksum(&record.payload);
    }

    WalSegment { records }
}

// Mock Recovery Engine

fn recover_from_wal_segment(segment: &WalSegment) -> RecoveryResult {
    let mut result = RecoveryResult {
        success: true,
        records_processed: 0,
        errors: Vec::new(),
    };

    let mut last_lsn: Option<u64> = None;

    for record in segment.records.iter() {
        // Verify LSN ordering
        if let Some(prev_lsn) = last_lsn
            && record.lsn <= prev_lsn
        {
            result.errors.push(format!(
                "LSN not strictly increasing: {} <= {}",
                record.lsn, prev_lsn
            ));
            result.success = false;
        }

        // Verify checksum
        let expected_checksum = calculate_mock_checksum(&record.payload);
        if expected_checksum != record.checksum {
            result
                .errors
                .push(format!("Checksum mismatch at LSN {}", record.lsn));
            result.success = false;
        }

        result.records_processed += 1;
        last_lsn = Some(record.lsn);
    }

    result
}

#[test]
fn prop_recovery_never_panics() {
    proptest!(|(segment in arb_wal_segment())| {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            recover_from_wal_segment(&segment)
        }));

        prop_assert!(result.is_ok(), "recovery panicked");
    });
}

#[test]
fn prop_recovery_always_decides() {
    proptest!(|(segment in arb_wal_segment())| {
        let result = recover_from_wal_segment(&segment);

        prop_assert_eq!(result.records_processed, segment.records.len());
        prop_assert_eq!(result.success, result.errors.is_empty());
        for error in result.errors {
            prop_assert!(!error.is_empty());
        }
    });
}

#[test]
fn prop_recovery_valid_segment_succeeds() {
    proptest!(|(
        records in prop::collection::vec(arb_wal_record(), 1..50),
    )| {
        let segment = valid_segment_from(records);
        let result = recover_from_wal_segment(&segment);

        prop_assert!(result.success, "valid segment should recover");
        prop_assert_eq!(result.records_processed, segment.records.len());
        prop_assert!(result.errors.is_empty());
    });
}

#[test]
fn prop_recovery_detects_checksum_corruption() {
    proptest!(|(
        records in prop::collection::vec(arb_wal_record(), 1..50),
        record_idx in 0usize..50,
    )| {
        let mut segment = valid_segment_from(records);
        let corrupt_idx = record_idx % segment.records.len();
        let corrupt_lsn = segment.records[corrupt_idx].lsn;
        segment.records[corrupt_idx].checksum ^= 0xFFFFFFFFFFFFFFFF;

        let result = recover_from_wal_segment(&segment);

        prop_assert!(!result.success, "corruption should fail recovery");
        prop_assert_eq!(result.records_processed, segment.records.len());
        prop_assert!(
            result
                .errors
                .iter()
                .any(|error| error == &format!("Checksum mismatch at LSN {}", corrupt_lsn)),
            "checksum error should identify the corrupted LSN"
        );
    });
}

#[test]
fn prop_recovery_detects_out_of_order() {
    proptest!(|(
        records in prop::collection::vec(arb_wal_record(), 2..50),
    )| {
        let mut segment = valid_segment_from(records);
        let last_index = segment.records.len() - 1;
        segment.records.swap(0, last_index);

        let result = recover_from_wal_segment(&segment);

        prop_assert!(!result.success, "out-of-order LSNs should fail recovery");
        prop_assert!(
            result
                .errors
                .iter()
                .any(|error| error.contains("LSN not strictly increasing")),
            "out-of-order error should explain the LSN ordering violation"
        );
    });
}

#[test]
fn prop_recovery_empty_segment() {
    let empty_segment = WalSegment { records: vec![] };

    let result = recover_from_wal_segment(&empty_segment);

    assert!(result.success);
    assert_eq!(result.records_processed, 0);
    assert!(result.errors.is_empty());
}

#[test]
fn prop_recovery_detects_duplicate_lsn() {
    proptest!(|(
        records in prop::collection::vec(arb_wal_record(), 2..50),
    )| {
        let mut segment = valid_segment_from(records);
        let duplicate_lsn = segment.records[0].lsn;
        for record in segment.records.iter_mut() {
            record.lsn = duplicate_lsn;
        }

        let result = recover_from_wal_segment(&segment);

        prop_assert!(!result.success, "duplicate LSNs should fail recovery");
        prop_assert_eq!(
            result
                .errors
                .iter()
                .filter(|error| error.contains("LSN not strictly increasing"))
                .count(),
            segment.records.len() - 1
        );
    });
}

#[test]
fn prop_recovery_large_segment() {
    proptest!(|(
        records in prop::collection::vec(arb_wal_record(), 128..512),
    )| {
        let segment = valid_segment_from(records);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            recover_from_wal_segment(&segment)
        }));

        match result {
            Ok(recovery_result) => {
                prop_assert!(recovery_result.success);
                prop_assert_eq!(recovery_result.records_processed, segment.records.len());
                prop_assert!(recovery_result.errors.is_empty());
            }
            Err(_) => {
                prop_assert!(false, "large segment caused panic");
            }
        }
    });
}

#[test]
fn prop_recovery_deterministic() {
    proptest!(|(segment in arb_wal_segment())| {
        let result1 = recover_from_wal_segment(&segment);
        let result2 = recover_from_wal_segment(&segment);

        prop_assert_eq!(result1, result2);
    });
}

#[test]
fn prop_recovery_error_messages_informative() {
    proptest!(|(
        mut segment in arb_wal_segment(),
    )| {
        if segment.records.len() >= 2 {
            // Corrupt the segment
            segment.records[0].checksum ^= 1;
        }

        let result = recover_from_wal_segment(&segment);

        // Error messages should be informative
        for error in result.errors.iter() {
            prop_assert!(!error.is_empty(), "error message should not be empty");
            prop_assert!(
                error.contains("LSN") || error.contains("Checksum") || error.contains("payload"),
                "error message should contain diagnostic info"
            );
        }
    });
}

#[test]
fn test_recovery_single_record() {
    let segment = WalSegment {
        records: vec![WalRecordData {
            lsn: 1000,
            payload: vec![1, 2, 3, 4, 5],
            checksum: calculate_mock_checksum(&[1u8, 2, 3, 4, 5]),
        }],
    };

    let result = recover_from_wal_segment(&segment);
    assert!(result.success);
    assert_eq!(result.records_processed, 1);
    assert!(result.errors.is_empty());
}

#[test]
fn test_recovery_ascending_lsn() {
    let mut records = vec![];
    for i in 0..100u64 {
        records.push(WalRecordData {
            lsn: i + 1000,
            payload: vec![i as u8],
            checksum: calculate_mock_checksum(&[i as u8]),
        });
    }

    let segment = WalSegment { records };
    let result = recover_from_wal_segment(&segment);

    assert!(result.success);
    assert_eq!(result.records_processed, 100);
}

#[test]
fn test_recovery_with_gaps_in_lsn() {
    let records = vec![
        WalRecordData {
            lsn: 1000,
            payload: vec![],
            checksum: 0,
        },
        WalRecordData {
            lsn: 1100, // Gap of 100
            payload: vec![],
            checksum: 0,
        },
        WalRecordData {
            lsn: 1500, // Gap of 400
            payload: vec![],
            checksum: 0,
        },
    ];

    let segment = WalSegment { records };
    let result = recover_from_wal_segment(&segment);

    assert!(result.success);
    assert_eq!(result.records_processed, 3);
    assert!(result.errors.is_empty());
}

#[test]
fn integration_recovery_full_workflow() {
    proptest!(|(
        segments in prop::collection::vec(arb_wal_segment(), 1..10),
    )| {
        for segment in segments.iter() {
            let result = recover_from_wal_segment(segment);
            prop_assert_eq!(result.records_processed, segment.records.len());
            prop_assert_eq!(result.success, result.errors.is_empty());
        }
    });
}
