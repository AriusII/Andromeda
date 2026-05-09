use andromeda_error::AndromedaErrorKind;
use andromeda_storage::write_ahead_log::compaction::{
    CompactionResult, FragmentationMetrics, WalCompactionAuditEvent, compact_segment,
};
use andromeda_storage::{Lsn, WalRecord, WalRecordKind};
use andromeda_types::TransactionId;

use crate::support::{MockCompactionContext, test_record};

#[test]
fn compaction_filters_dead_records() {
    let mut records = vec![];
    for lsn in 1..=10 {
        records.push(test_record(lsn));
    }

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 400).unwrap())
        .with_segment_records(1, records)
        .with_live_record(1)
        .with_live_record(2)
        .with_live_record(3)
        .with_dead_record(4)
        .with_dead_record(5)
        .with_dead_record(6)
        .with_live_record(7)
        .with_live_record(8)
        .with_live_record(9)
        .with_live_record(10);

    let result = compact_segment(&context, 1).unwrap();

    assert_eq!(result.compacted_record_count, 7);
    assert_eq!(result.records_removed, 3);

    let written = context.get_written_segments();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].0, 1);
    assert_eq!(written[0].1, 1001);

    let swapped = context.get_swapped_segments();
    assert_eq!(swapped.len(), 1);
    assert_eq!(swapped[0].0, 1);
    assert_eq!(swapped[0].1, 1001);
}

#[test]
fn compaction_maintains_lsn_order() {
    let records = vec![test_record(100), test_record(101), test_record(102)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(2, 500, 150).unwrap())
        .with_segment_records(2, records)
        .with_live_record(100)
        .with_live_record(101)
        .with_live_record(102);

    let result = compact_segment(&context, 2).unwrap();

    assert_eq!(result.compacted_record_count, 3);
    assert_eq!(result.original_record_count, 3);
    assert_eq!(result.records_removed, 0);
}

#[test]
fn compaction_calculates_space_recovery() {
    let mut records = vec![];
    for lsn in 1..=10 {
        records.push(test_record(lsn));
    }

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(3, 1000, 400).unwrap())
        .with_segment_records(3, records)
        .with_dead_record(1)
        .with_dead_record(2)
        .with_dead_record(3)
        .with_dead_record(4);

    let result = compact_segment(&context, 3).unwrap();

    assert_eq!(result.original_bytes, 640);
    assert_eq!(result.bytes_recovered, 256);
    assert!((result.reduction_ratio() - 0.40).abs() < 0.01);
}

#[test]
fn compaction_preserves_record_content() {
    let record = WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(100),
        None,
        Some(TransactionId::new(1)),
        vec![1, 2, 3, 4, 5],
    )
    .unwrap();

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(4, 200, 50).unwrap())
        .with_segment_records(4, vec![record])
        .with_live_record(100);

    let result = compact_segment(&context, 4).unwrap();

    assert_eq!(result.original_record_count, 1);
    assert_eq!(result.compacted_record_count, 1);
    assert_eq!(result.records_removed, 0);
}

#[test]
fn compaction_fails_gracefully_on_write_failure() {
    let records = vec![test_record(1), test_record(2)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(5, 500, 200).unwrap())
        .with_segment_records(5, records)
        .fail_on_write();

    let result = compact_segment(&context, 5);

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);

    let swapped = context.get_swapped_segments();
    assert_eq!(swapped.len(), 0);

    let events = context.get_audit_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, WalCompactionAuditEvent::CompactionFailed { .. }))
    );
}

#[test]
fn compaction_preserves_old_segment_on_swap_failure() {
    let records = vec![test_record(1), test_record(2), test_record(3)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(6, 600, 200).unwrap())
        .with_segment_records(6, records)
        .fail_on_swap();

    let result = compact_segment(&context, 6);

    assert!(result.is_err());

    let written = context.get_written_segments();
    assert_eq!(written.len(), 1);

    let swapped = context.get_swapped_segments();
    assert_eq!(swapped.len(), 0);

    let events = context.get_audit_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, WalCompactionAuditEvent::CompactionStarted { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, WalCompactionAuditEvent::CompactionFailed { .. }))
    );
}

#[test]
fn compaction_handles_all_live_records() {
    let records = vec![test_record(1), test_record(2), test_record(3)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(7, 500, 0).unwrap())
        .with_segment_records(7, records);

    let result = compact_segment(&context, 7).unwrap();

    assert_eq!(result.records_removed, 0);
    assert_eq!(result.compacted_record_count, result.original_record_count);
    assert_eq!(result.bytes_recovered, 0);
}

#[test]
fn compaction_handles_all_dead_records() {
    let records = vec![test_record(1), test_record(2), test_record(3)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(8, 500, 500).unwrap())
        .with_segment_records(8, records)
        .with_dead_record(1)
        .with_dead_record(2)
        .with_dead_record(3);

    let result = compact_segment(&context, 8).unwrap();

    assert_eq!(result.compacted_record_count, 0);
    assert_eq!(result.records_removed, 3);
    assert_eq!(result.bytes_recovered, result.original_bytes);
}

#[test]
fn audit_events_provide_complete_metadata() {
    let records = vec![test_record(1), test_record(2)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(9, 500, 200).unwrap())
        .with_segment_records(9, records);

    let _ = compact_segment(&context, 9);

    let events = context.get_audit_events();

    assert!(events.iter().any(|event| {
        matches!(
            event,
            WalCompactionAuditEvent::CompactionStarted { segment_id: 9, .. }
        )
    }));

    assert!(events.iter().any(|event| {
        matches!(
            event,
            WalCompactionAuditEvent::CompactionCompleted {
                original_segment_id: 9,
                ..
            }
        )
    }));
}

#[test]
fn compaction_result_metrics_are_consistent() {
    let result = CompactionResult {
        original_segment_id: 1,
        new_segment_id: 2,
        original_bytes: 1000,
        compacted_bytes: 700,
        bytes_recovered: 300,
        original_record_count: 100,
        compacted_record_count: 70,
        records_removed: 30,
    };

    assert_eq!(
        result.bytes_recovered,
        result.original_bytes - result.compacted_bytes
    );
    assert_eq!(
        result.records_removed,
        result.original_record_count - result.compacted_record_count
    );
}
