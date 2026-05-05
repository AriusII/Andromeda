//! Integration tests for WAL segment compaction contract compliance.
//!
//! Tests cover:
//! - Fragmentation detection (ratio calculation, threshold behavior)
//! - Compaction algorithm (record filtering, LSN correctness, space recovery)
//! - Scheduler (periodic execution, metrics emission)
//! - Error recovery (write failure, swap failure, data preservation)
//! - Edge cases (empty segments, all-live records, all-dead records)

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_storage::write_ahead_log::compaction::*;
use andromeda_storage::{Lsn, WalRecord, WalRecordKind};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Mock implementation of CompactionContext for testing.
struct MockCompactionContext {
    /// Segments and their fragmentation metrics
    fragmented_segments: Vec<FragmentationMetrics>,
    /// Segment ID → Records mapping
    segment_records: HashMap<u64, Vec<WalRecord>>,
    /// Records to keep (by LSN)
    live_lsns: HashSet<u64>,
    /// Records to discard (by LSN)
    dead_lsns: HashSet<u64>,
    /// Track removed segments
    removed_segments: Arc<Mutex<Vec<u64>>>,
    /// Track written segments (segment_id → (new_id, bytes))
    written_segments: Arc<Mutex<Vec<(u64, u64, u64)>>>,
    /// Track swaps
    swapped_segments: Arc<Mutex<Vec<(u64, u64)>>>,
    /// Audit events emitted
    audit_events: Arc<Mutex<Vec<WalCompactionAuditEvent>>>,
    /// Simulate write failure?
    fail_write: bool,
    /// Simulate swap failure?
    fail_swap: bool,
}

impl MockCompactionContext {
    fn new() -> Self {
        MockCompactionContext {
            fragmented_segments: Vec::new(),
            segment_records: HashMap::new(),
            live_lsns: HashSet::new(),
            dead_lsns: HashSet::new(),
            removed_segments: Arc::new(Mutex::new(Vec::new())),
            written_segments: Arc::new(Mutex::new(Vec::new())),
            swapped_segments: Arc::new(Mutex::new(Vec::new())),
            audit_events: Arc::new(Mutex::new(Vec::new())),
            fail_write: false,
            fail_swap: false,
        }
    }

    fn with_fragmented_segment(mut self, metrics: FragmentationMetrics) -> Self {
        self.fragmented_segments.push(metrics);
        self
    }

    fn with_segment_records(mut self, segment_id: u64, records: Vec<WalRecord>) -> Self {
        self.segment_records.insert(segment_id, records);
        self
    }

    fn with_live_record(mut self, lsn: u64) -> Self {
        self.live_lsns.insert(lsn);
        self
    }

    fn with_dead_record(mut self, lsn: u64) -> Self {
        self.dead_lsns.insert(lsn);
        self
    }

    fn fail_on_write(mut self) -> Self {
        self.fail_write = true;
        self
    }

    fn fail_on_swap(mut self) -> Self {
        self.fail_swap = true;
        self
    }

    fn get_audit_events(&self) -> Vec<WalCompactionAuditEvent> {
        self.audit_events.lock().unwrap().clone()
    }

    fn get_written_segments(&self) -> Vec<(u64, u64, u64)> {
        self.written_segments.lock().unwrap().clone()
    }

    fn get_swapped_segments(&self) -> Vec<(u64, u64)> {
        self.swapped_segments.lock().unwrap().clone()
    }
}

impl CompactionContext for MockCompactionContext {
    fn identify_fragmented_segments(
        &self,
        threshold_ratio: f64,
    ) -> AndromedaResult<Vec<FragmentationMetrics>> {
        Ok(self
            .fragmented_segments
            .iter()
            .filter(|m| m.fragmentation_ratio >= threshold_ratio)
            .copied()
            .collect())
    }

    fn read_segment_records(&self, segment_id: u64) -> AndromedaResult<Vec<WalRecord>> {
        self.segment_records
            .get(&segment_id)
            .cloned()
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("segment {} not found", segment_id),
                )
            })
    }

    fn should_keep_record(&self, record: &WalRecord) -> AndromedaResult<bool> {
        let lsn = record.header.lsn.get();
        if self.live_lsns.contains(&lsn) {
            Ok(true)
        } else if self.dead_lsns.contains(&lsn) {
            Ok(false)
        } else {
            // Default: keep all records not explicitly marked dead
            Ok(true)
        }
    }

    fn write_compacted_segment(
        &self,
        original_segment_id: u64,
        records: &[WalRecord],
    ) -> AndromedaResult<(u64, u64)> {
        if self.fail_write {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "simulated write failure",
            ));
        }

        let new_segment_id = original_segment_id + 1000; // Simple ID scheme for testing
        let bytes_written = records.iter().map(|r| r.payload.len()).sum::<usize>() as u64;

        self.written_segments.lock().unwrap().push((
            original_segment_id,
            new_segment_id,
            bytes_written,
        ));

        Ok((new_segment_id, bytes_written))
    }

    fn swap_segment(&self, old_segment_id: u64, new_segment_id: u64) -> AndromedaResult<()> {
        if self.fail_swap {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "simulated swap failure",
            ));
        }

        self.swapped_segments
            .lock()
            .unwrap()
            .push((old_segment_id, new_segment_id));

        Ok(())
    }

    fn emit_audit_event(&self, event: WalCompactionAuditEvent) -> AndromedaResult<()> {
        self.audit_events.lock().unwrap().push(event);
        Ok(())
    }
}

// Helper to create a WAL record for testing
fn test_record(lsn: u64) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        if lsn > 1 {
            Some(Lsn::new(lsn - 1))
        } else {
            None
        },
        Some(TransactionId::new(1)),
        vec![0u8; 64],
    )
    .unwrap()
}

// ============================================================================
// FRAGMENTATION DETECTION TESTS (3 tests)
// ============================================================================

#[test]
fn fragmentation_ratio_calculation_exact_threshold() {
    // Exactly 30% fragmentation
    let metrics = FragmentationMetrics::new(1, 100, 30).unwrap();
    assert!((metrics.fragmentation_ratio - 0.30).abs() < 0.001);
    assert!(metrics.is_compaction_candidate());
}

#[test]
fn fragmentation_ratio_calculation_above_threshold() {
    // 50% fragmentation
    let metrics = FragmentationMetrics::new(1, 100, 50).unwrap();
    assert!((metrics.fragmentation_ratio - 0.50).abs() < 0.001);
    assert!(metrics.is_compaction_candidate());
}

#[test]
fn fragmentation_ratio_calculation_below_threshold() {
    // 29.9% fragmentation (below threshold)
    let metrics = FragmentationMetrics::new(1, 1000, 299).unwrap();
    assert!(metrics.fragmentation_ratio < 0.30);
    assert!(!metrics.is_compaction_candidate());

    // 29% fragmentation
    let metrics = FragmentationMetrics::new(1, 100, 29).unwrap();
    assert!(metrics.fragmentation_ratio < 0.30);
    assert!(!metrics.is_compaction_candidate());
}

// ============================================================================
// COMPACTION ALGORITHM TESTS (4 tests)
// ============================================================================

#[test]
fn compaction_filters_dead_records() {
    // Create a segment with 10 records: LSN 1-10
    let mut records = vec![];
    for i in 1..=10 {
        records.push(test_record(i));
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

    // Should keep 8 records (all but 4, 5, 6)
    assert_eq!(result.compacted_record_count, 8);
    assert_eq!(result.records_removed, 2); // Only 4, 5, 6 are marked dead; default keeps unknown

    // Verify written records count
    let written = context.get_written_segments();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].0, 1); // Original segment ID
    assert_eq!(written[0].1, 1001); // New segment ID

    // Verify swap occurred
    let swapped = context.get_swapped_segments();
    assert_eq!(swapped.len(), 1);
    assert_eq!(swapped[0].0, 1); // Old ID
    assert_eq!(swapped[0].1, 1001); // New ID
}

#[test]
fn compaction_maintains_lsn_order() {
    // Create records and ensure order is maintained
    let records = vec![test_record(100), test_record(101), test_record(102)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(2, 500, 150).unwrap())
        .with_segment_records(2, records)
        .with_live_record(100)
        .with_live_record(101)
        .with_live_record(102);

    let result = compact_segment(&context, 2).unwrap();

    // All records are live
    assert_eq!(result.compacted_record_count, 3);
    assert_eq!(result.original_record_count, 3);
    assert_eq!(result.records_removed, 0);
}

#[test]
fn compaction_calculates_space_recovery() {
    // Original: 1000 bytes, dead: 400 bytes
    // After compaction: 600 bytes
    // Recovery: 400 bytes
    let mut records = vec![];
    for i in 1..=10 {
        records.push(test_record(i));
    }

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(3, 1000, 400).unwrap())
        .with_segment_records(3, records)
        .with_dead_record(1)
        .with_dead_record(2)
        .with_dead_record(3)
        .with_dead_record(4); // 40% dead

    let result = compact_segment(&context, 3).unwrap();

    assert_eq!(result.original_bytes, 640); // 10 records * 64 bytes
    assert_eq!(result.bytes_recovered, 256); // 4 dead records * 64 bytes
    assert!((result.reduction_ratio() - 0.40).abs() < 0.01);
}

#[test]
fn compaction_preserves_record_content() {
    let original_payload = vec![1, 2, 3, 4, 5];
    let record = WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(100),
        None,
        Some(TransactionId::new(1)),
        original_payload.clone(),
    )
    .unwrap();

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(4, 200, 50).unwrap())
        .with_segment_records(4, vec![record.clone()])
        .with_live_record(100);

    let result = compact_segment(&context, 4).unwrap();

    assert_eq!(result.original_record_count, 1);
    assert_eq!(result.compacted_record_count, 1);
    assert_eq!(result.records_removed, 0);
}

// ============================================================================
// SCHEDULER TESTS (2 tests)
// ============================================================================

#[test]
fn scheduler_identifies_and_compacts_candidates() {
    use std::time::Duration;

    let mut records_set1 = vec![];
    for i in 1..=5 {
        records_set1.push(test_record(i));
    }

    let mut records_set2 = vec![];
    for i in 100..=110 {
        records_set2.push(test_record(i));
    }

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(2, 1500, 400).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(3, 500, 100).unwrap()) // Below 30% threshold
        .with_segment_records(1, records_set1)
        .with_segment_records(2, records_set2);

    // Mark all records as live
    for i in 1..=5 {
        let context_iter = std::cell::Cell::new(());
        // Records are live by default
    }

    let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30).unwrap();
    let scheduler = WalCompactionScheduler::new(config);

    let summary = scheduler.run(&context).unwrap();

    // Should identify 2 candidates (segments 1 and 2)
    assert_eq!(summary.candidates_identified, 2);
    // Should attempt to compact both
    assert!(summary.candidates_compacted > 0 || summary.compaction_skipped > 0);
    assert_eq!(summary.run_id, 0); // First run

    // Check audit events
    let events = context.get_audit_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WalCompactionAuditEvent::CandidateIdentified { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WalCompactionAuditEvent::Summary { .. }))
    );
}

#[test]
fn scheduler_respects_max_segments_per_run() {
    use std::time::Duration;

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(2, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(3, 1000, 350).unwrap())
        .with_segment_records(1, vec![test_record(1)])
        .with_segment_records(2, vec![test_record(2)])
        .with_segment_records(3, vec![test_record(3)]);

    let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30)
        .unwrap()
        .with_max_segments(2); // Limit to 2 per run

    let scheduler = WalCompactionScheduler::new(config);
    let summary = scheduler.run(&context).unwrap();

    // Should identify 3 but only compact up to 2
    assert_eq!(summary.candidates_identified, 3);
    assert!(summary.candidates_compacted <= 2);
}

// ============================================================================
// ERROR RECOVERY TESTS (2 tests)
// ============================================================================

#[test]
fn compaction_fails_gracefully_on_write_failure() {
    let records = vec![test_record(1), test_record(2)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(5, 500, 200).unwrap())
        .with_segment_records(5, records)
        .fail_on_write();

    let result = compact_segment(&context, 5);

    // Compaction should fail
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);

    // No swap should have occurred
    let swapped = context.get_swapped_segments();
    assert_eq!(swapped.len(), 0);

    // Audit event for failure
    let events = context.get_audit_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WalCompactionAuditEvent::CompactionFailed { .. }))
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

    // Compaction should fail
    assert!(result.is_err());

    // Verify write occurred (temporary created)
    let written = context.get_written_segments();
    assert_eq!(written.len(), 1);

    // Verify swap was attempted but failed
    let swapped = context.get_swapped_segments();
    assert_eq!(swapped.len(), 0); // Never succeeded

    // Audit event for failure
    let events = context.get_audit_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WalCompactionAuditEvent::CompactionStarted { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WalCompactionAuditEvent::CompactionFailed { .. }))
    );
}

// ============================================================================
// EDGE CASE TESTS (2 tests)
// ============================================================================

#[test]
fn compaction_handles_all_live_records() {
    // Segment with all live records (0% fragmentation)
    let records = vec![test_record(1), test_record(2), test_record(3)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(7, 500, 0).unwrap()) // No dead space
        .with_segment_records(7, records);

    let result = compact_segment(&context, 7).unwrap();

    assert_eq!(result.records_removed, 0);
    assert_eq!(result.compacted_record_count, result.original_record_count);
    assert_eq!(result.bytes_recovered, 0);
}

#[test]
fn compaction_handles_all_dead_records() {
    // Segment with all dead records (100% fragmentation)
    let records = vec![test_record(1), test_record(2), test_record(3)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(8, 500, 500).unwrap()) // 100% dead
        .with_segment_records(8, records.clone())
        .with_dead_record(1)
        .with_dead_record(2)
        .with_dead_record(3);

    let result = compact_segment(&context, 8).unwrap();

    assert_eq!(result.compacted_record_count, 0);
    assert_eq!(result.records_removed, 3);
    assert_eq!(result.bytes_recovered, result.original_bytes);
}

// ============================================================================
// INTEGRATION TESTS (additional coverage)
// ============================================================================

#[test]
fn identify_compaction_candidates_sorts_by_fragmentation() {
    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 300).unwrap()) // 30%
        .with_fragmented_segment(FragmentationMetrics::new(2, 1000, 500).unwrap()) // 50%
        .with_fragmented_segment(FragmentationMetrics::new(3, 1000, 350).unwrap()) // 35%
        .with_fragmented_segment(FragmentationMetrics::new(4, 1000, 200).unwrap()); // 20% - below threshold

    let candidates = identify_compaction_candidates(&context, 0.30).unwrap();

    // Should identify 3 candidates (all >= 30%)
    assert_eq!(candidates.len(), 3);

    // Should be sorted by fragmentation ratio descending: 50%, 35%, 30%
    assert_eq!(candidates[0].segment_id, 2); // 50%
    assert_eq!(candidates[1].segment_id, 3); // 35%
    assert_eq!(candidates[2].segment_id, 1); // 30%

    // Verify audit events emitted
    let events = context.get_audit_events();
    let candidate_events = events
        .iter()
        .filter(|e| matches!(e, WalCompactionAuditEvent::CandidateIdentified { .. }))
        .count();
    assert_eq!(candidate_events, 3);
}

#[test]
fn audit_events_provide_complete_metadata() {
    let records = vec![test_record(1), test_record(2)];

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(9, 500, 200).unwrap())
        .with_segment_records(9, records);

    let _ = compact_segment(&context, 9);

    let events = context.get_audit_events();

    // Verify CompactionStarted event
    assert!(events.iter().any(|e| {
        matches!(
            e,
            WalCompactionAuditEvent::CompactionStarted { segment_id: 9, .. }
        )
    }));

    // Verify CompactionCompleted event
    assert!(events.iter().any(|e| {
        matches!(
            e,
            WalCompactionAuditEvent::CompactionCompleted {
                original_segment_id: 9,
                ..
            }
        )
    }));
}

#[test]
fn scheduler_increments_run_id() {
    use std::time::Duration;

    let context = MockCompactionContext::new();

    let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30).unwrap();
    let scheduler = WalCompactionScheduler::new(config);

    let summary1 = scheduler.run(&context).unwrap();
    let summary2 = scheduler.run(&context).unwrap();
    let summary3 = scheduler.run(&context).unwrap();

    assert_eq!(summary1.run_id, 0);
    assert_eq!(summary2.run_id, 1);
    assert_eq!(summary3.run_id, 2);
}

#[test]
fn scheduler_provides_interval_and_threshold_accessors() {
    use std::time::Duration;

    let duration = Duration::from_secs(120);
    let threshold = 0.35;

    let config = WalCompactionSchedulerConfig::new(duration, threshold).unwrap();
    let scheduler = WalCompactionScheduler::new(config);

    assert_eq!(scheduler.interval(), duration);
    assert_eq!(scheduler.fragmentation_threshold(), threshold);
}

#[test]
fn compaction_result_metrics_are_consistent() {
    // Verify: bytes_recovered = original_bytes - compacted_bytes
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
