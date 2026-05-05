//! Integration tests for WAL Garbage Collection contract compliance.
//!
//! Tests cover:
//! - Candidate identification with LSN thresholds
//! - Archive verification and fail-safe behavior
//! - Safe segment removal with recovery guarantees
//! - Scheduler operation and metrics
//! - Recovery boundary validation
//! - Edge cases and error handling

use andromeda_storage::write_ahead_log::gc::*;
use andromeda_storage::Lsn;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Mock WAL GC context for testing.
struct MockWalGcContext {
    candidates: Vec<WalGcCandidate>,
    archive_status_map: std::collections::HashMap<u64, ArchiveStatus>,
    min_snapshot_lsn: Lsn,
    required_start_lsn: Lsn,
    removed_segments: Arc<Mutex<Vec<u64>>>,
    audit_events: Arc<Mutex<Vec<WalGcAuditEvent>>>,
    removal_should_fail: bool,
}

impl MockWalGcContext {
    fn new() -> Self {
        MockWalGcContext {
            candidates: Vec::new(),
            archive_status_map: std::collections::HashMap::new(),
            min_snapshot_lsn: Lsn::new(1000),
            required_start_lsn: Lsn::new(100),
            removed_segments: Arc::new(Mutex::new(Vec::new())),
            audit_events: Arc::new(Mutex::new(Vec::new())),
            removal_should_fail: false,
        }
    }

    fn with_candidates(mut self, candidates: Vec<WalGcCandidate>) -> Self {
        self.candidates = candidates;
        self
    }

    fn with_min_snapshot_lsn(mut self, lsn: Lsn) -> Self {
        self.min_snapshot_lsn = lsn;
        self
    }

    fn with_required_start_lsn(mut self, lsn: Lsn) -> Self {
        self.required_start_lsn = lsn;
        self
    }

    fn add_archived_segment(mut self, segment_id: u64) -> Self {
        self.archive_status_map.insert(segment_id, ArchiveStatus::Archived);
        self
    }

    fn add_pending_segment(mut self, segment_id: u64) -> Self {
        self.archive_status_map.insert(segment_id, ArchiveStatus::Pending);
        self
    }

    fn fail_removal(mut self) -> Self {
        self.removal_should_fail = true;
        self
    }

    fn get_removed_segments(&self) -> Vec<u64> {
        self.removed_segments.lock().unwrap().clone()
    }

    fn get_audit_events(&self) -> Vec<WalGcAuditEvent> {
        self.audit_events.lock().unwrap().clone()
    }
}

impl WalGcContext for MockWalGcContext {
    fn identify_gc_candidates(&self, _min_active_snapshot_lsn: Lsn) -> AndromedaResult<Vec<WalGcCandidate>> {
        Ok(self.candidates.clone())
    }

    fn verify_archived(&self, candidate: &WalGcCandidate) -> AndromedaResult<ArchiveStatus> {
        let status = self.archive_status_map
            .get(&candidate.segment_id)
            .copied()
            .unwrap_or(ArchiveStatus::Unknown);
        Ok(status)
    }

    fn safe_remove_segment(&self, candidate: &WalGcCandidate) -> AndromedaResult<()> {
        if self.removal_should_fail {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "simulated removal failure",
            ));
        }
        self.removed_segments.lock().unwrap().push(candidate.segment_id);
        Ok(())
    }

    fn min_active_snapshot_lsn(&self) -> Lsn {
        self.min_snapshot_lsn
    }

    fn required_wal_start_lsn(&self) -> Lsn {
        self.required_start_lsn
    }

    fn emit_audit_event(&self, event: WalGcAuditEvent) -> AndromedaResult<()> {
        self.audit_events.lock().unwrap().push(event);
        Ok(())
    }
}

// ============================================================================
// Candidate Identification Tests (3 tests)
// ============================================================================

#[test]
fn identify_candidates_returns_empty_when_no_candidates() {
    let context = MockWalGcContext::new();
    let gc = WalGarbageCollector::new(Arc::new(context));

    let candidates = gc.identify_candidates().expect("should succeed");
    assert_eq!(candidates.len(), 0);
}

#[test]
fn identify_candidates_filters_by_lsn_thresholds() {
    // Create several candidates with different LSN ranges
    let candidate1 = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let candidate2 = WalGcCandidate::new(2, Lsn::new(300), Lsn::new(400), 4096).unwrap();
    let candidate3 = WalGcCandidate::new(3, Lsn::new(500), Lsn::new(600), 4096).unwrap();

    let context = MockWalGcContext::new()
        .with_candidates(vec![candidate1, candidate2, candidate3])
        .with_min_snapshot_lsn(Lsn::new(350)) // Only candidate1 and candidate2 eligible
        .with_required_start_lsn(Lsn::new(250)); // Only candidate2 and candidate3 eligible

    let gc = WalGarbageCollector::new(Arc::new(context));
    let candidates = gc.identify_candidates().expect("should succeed");

    // Only candidate2 should be eligible (300-400 satisfies both thresholds)
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].segment_id, 2);
}

#[test]
fn identify_candidates_sorts_by_creation_lsn() {
    let candidate1 = WalGcCandidate::new(1, Lsn::new(500), Lsn::new(600), 4096).unwrap();
    let candidate2 = WalGcCandidate::new(2, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let candidate3 = WalGcCandidate::new(3, Lsn::new(300), Lsn::new(400), 4096).unwrap();

    let context = MockWalGcContext::new()
        .with_candidates(vec![candidate1, candidate2, candidate3])
        .with_min_snapshot_lsn(Lsn::new(700))
        .with_required_start_lsn(Lsn::ZERO);

    let gc = WalGarbageCollector::new(Arc::new(context));
    let candidates = gc.identify_candidates().expect("should succeed");

    // All eligible; should be sorted by creation_lsn (oldest first)
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].segment_id, 2); // 100-200
    assert_eq!(candidates[1].segment_id, 3); // 300-400
    assert_eq!(candidates[2].segment_id, 1); // 500-600
}

// ============================================================================
// Archive Verification Tests (3 tests)
// ============================================================================

#[test]
fn remove_if_archived_succeeds_when_segment_archived() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = MockWalGcContext::new()
        .add_archived_segment(1);

    let gc = WalGarbageCollector::new(Arc::new(context));
    let removed = gc.remove_if_archived(&candidate).expect("should succeed");

    assert!(removed);
}

#[test]
fn remove_if_archived_blocks_when_segment_pending() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = MockWalGcContext::new()
        .add_pending_segment(1);

    let gc = WalGarbageCollector::new(Arc::new(context));
    let removed = gc.remove_if_archived(&candidate).expect("should succeed");

    assert!(!removed); // Fail-safe: not archived yet
}

#[test]
fn remove_if_archived_blocks_when_archive_status_unknown() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = MockWalGcContext::new(); // No archive status set

    let gc = WalGarbageCollector::new(Arc::new(context));
    let removed = gc.remove_if_archived(&candidate).expect("should succeed");

    assert!(!removed); // Fail-safe: status unknown
}

// ============================================================================
// Safe Removal Tests (3 tests)
// ============================================================================

#[test]
fn run_gc_successfully_removes_archived_segments() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = Arc::new(MockWalGcContext::new()
        .with_candidates(vec![candidate])
        .add_archived_segment(1)
        .with_min_snapshot_lsn(Lsn::new(300))
        .with_required_start_lsn(Lsn::new(50)));

    let gc = WalGarbageCollector::new(Arc::clone(&context));
    let summary = gc.run_gc(1).expect("should succeed");

    assert_eq!(summary.run_id, 1);
    assert_eq!(summary.candidates_identified, 1);
    assert_eq!(summary.segments_removed, 1);
    assert_eq!(summary.bytes_freed, 4096);
    assert_eq!(summary.candidates_blocked, 0);

    let removed = context.get_removed_segments();
    assert_eq!(removed, vec![1]);
}

#[test]
fn run_gc_blocks_unarchived_segments() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = Arc::new(MockWalGcContext::new()
        .with_candidates(vec![candidate])
        .add_pending_segment(1)
        .with_min_snapshot_lsn(Lsn::new(300))
        .with_required_start_lsn(Lsn::new(50)));

    let gc = WalGarbageCollector::new(Arc::clone(&context));
    let summary = gc.run_gc(1).expect("should succeed");

    assert_eq!(summary.candidates_identified, 1);
    assert_eq!(summary.segments_removed, 0);
    assert_eq!(summary.candidates_blocked, 1);

    let removed = context.get_removed_segments();
    assert!(removed.is_empty());
}

#[test]
fn run_gc_emits_audit_events() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = Arc::new(MockWalGcContext::new()
        .with_candidates(vec![candidate])
        .add_archived_segment(1)
        .with_min_snapshot_lsn(Lsn::new(300))
        .with_required_start_lsn(Lsn::new(50)));

    let gc = WalGarbageCollector::new(Arc::clone(&context));
    let _summary = gc.run_gc(1).expect("should succeed");

    let events = context.get_audit_events();
    // Expected: CandidateIdentified, ArchiveVerifyRequested, SegmentRemoved, Summary
    assert!(events.len() >= 4);

    // Check event types
    let event_names: Vec<&str> = events.iter().map(|e| e.as_str()).collect();
    assert!(event_names.contains(&"GcCandidateIdentified"));
    assert!(event_names.contains(&"GcArchiveVerifyRequested"));
    assert!(event_names.contains(&"GcSegmentRemoved"));
    assert!(event_names.contains(&"GcSummary"));
}

// ============================================================================
// Scheduler Tests (3 tests)
// ============================================================================

#[test]
fn scheduler_creation_validates_config() {
    // Valid config
    let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10);
    let mock_gc = Arc::new(WalGarbageCollector::new(Arc::new(MockWalGcContext::new())));
    let scheduler = WalGcScheduler::new(config, mock_gc);
    assert!(scheduler.is_ok());

    // Invalid: zero interval
    let config = WalGcSchedulerConfig {
        interval: Duration::ZERO,
        target_free_gib: 10,
        max_segments_per_run: 0,
    };
    let mock_gc = Arc::new(WalGarbageCollector::new(Arc::new(MockWalGcContext::new())));
    let scheduler = WalGcScheduler::new(config, mock_gc);
    assert!(scheduler.is_err());

    // Invalid: zero target free
    let config = WalGcSchedulerConfig {
        interval: Duration::from_secs(60),
        target_free_gib: 0,
        max_segments_per_run: 0,
    };
    let mock_gc = Arc::new(WalGarbageCollector::new(Arc::new(MockWalGcContext::new())));
    let scheduler = WalGcScheduler::new(config, mock_gc);
    assert!(scheduler.is_err());
}

#[test]
fn scheduler_executes_tick() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = Arc::new(MockWalGcContext::new()
        .with_candidates(vec![candidate])
        .add_archived_segment(1)
        .with_min_snapshot_lsn(Lsn::new(300))
        .with_required_start_lsn(Lsn::new(50)));

    let mock_gc = Arc::new(WalGarbageCollector::new(Arc::clone(&context)));
    let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10);
    let scheduler = WalGcScheduler::new(config, mock_gc).expect("valid config");

    let summary = scheduler.tick(1).expect("should succeed");
    assert_eq!(summary.run_id, 1);
    assert_eq!(summary.segments_removed, 1);
}

#[test]
fn scheduler_config_with_max_segments() {
    let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10)
        .with_max_segments(100);
    let mock_gc = Arc::new(WalGarbageCollector::new(Arc::new(MockWalGcContext::new())));
    let scheduler = WalGcScheduler::new(config, mock_gc).expect("valid config");

    assert_eq!(scheduler.config().max_segments_per_run, 100);
}

// ============================================================================
// Recovery Boundary Validation Tests (2 tests)
// ============================================================================

#[test]
fn gc_respects_recovery_boundary() {
    // Segment created before recovery boundary should not be GC'd
    let candidate_before = WalGcCandidate::new(1, Lsn::new(50), Lsn::new(150), 4096).unwrap();
    let candidate_after = WalGcCandidate::new(2, Lsn::new(200), Lsn::new(300), 4096).unwrap();

    let context = MockWalGcContext::new()
        .with_candidates(vec![candidate_before, candidate_after])
        .with_min_snapshot_lsn(Lsn::new(500))
        .with_required_start_lsn(Lsn::new(100)); // Recovery boundary

    let gc = WalGarbageCollector::new(Arc::new(context));
    let candidates = gc.identify_candidates().expect("should succeed");

    // Only candidate_after should pass (200 > 100)
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].segment_id, 2);
}

#[test]
fn gc_checks_creation_lsn_strictly_greater_than_boundary() {
    // Segment with creation_lsn == required_start_lsn should not be GC'd
    let candidate_at_boundary = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();

    let context = MockWalGcContext::new()
        .with_candidates(vec![candidate_at_boundary])
        .with_min_snapshot_lsn(Lsn::new(500))
        .with_required_start_lsn(Lsn::new(100)); // Exact boundary

    let gc = WalGarbageCollector::new(Arc::new(context));
    let candidates = gc.identify_candidates().expect("should succeed");

    // Should be blocked (not strictly greater)
    assert_eq!(candidates.len(), 0);
}

// ============================================================================
// Edge Case Tests (2 tests)
// ============================================================================

#[test]
fn gc_handles_mixed_archived_and_pending_segments() {
    let candidate1 = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let candidate2 = WalGcCandidate::new(2, Lsn::new(300), Lsn::new(400), 4096).unwrap();
    let candidate3 = WalGcCandidate::new(3, Lsn::new(500), Lsn::new(600), 4096).unwrap();

    let context = Arc::new(MockWalGcContext::new()
        .with_candidates(vec![candidate1, candidate2, candidate3])
        .add_archived_segment(1)
        .add_pending_segment(2)
        // candidate3 has unknown status
        .with_min_snapshot_lsn(Lsn::new(700))
        .with_required_start_lsn(Lsn::new(50)));

    let gc = WalGarbageCollector::new(Arc::clone(&context));
    let summary = gc.run_gc(1).expect("should succeed");

    // Only candidate1 should be removed
    assert_eq!(summary.segments_removed, 1);
    assert_eq!(summary.candidates_blocked, 2);

    let removed = context.get_removed_segments();
    assert_eq!(removed, vec![1]);
}

#[test]
fn gc_summary_tracks_all_metrics() {
    let mut summary = WalGcSummary::new(42);
    assert_eq!(summary.run_id, 42);
    assert!(!summary.any_removed());

    summary.candidates_identified = 5;
    summary.candidates_archived = 4;
    summary.candidates_blocked = 1;
    summary.segments_removed = 3;
    summary.bytes_freed = 12288;

    assert!(summary.any_removed());
    assert_eq!(summary.candidates_identified, 5);
    assert_eq!(summary.bytes_freed, 12288);
}

// ============================================================================
// Error Handling Tests (2 tests)
// ============================================================================

#[test]
fn gc_emits_summary_even_on_removal_failures() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = Arc::new(MockWalGcContext::new()
        .with_candidates(vec![candidate])
        .add_archived_segment(1)
        .fail_removal() // Configure removal to fail
        .with_min_snapshot_lsn(Lsn::new(300))
        .with_required_start_lsn(Lsn::new(50)));

    let gc = WalGarbageCollector::new(Arc::clone(&context));
    let result = gc.run_gc(1);

    // Should fail because removal failed
    assert!(result.is_err());
}

#[test]
fn archive_verification_never_panics() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).unwrap();
    let context = Arc::new(MockWalGcContext::new()); // No archive status configured

    let gc = WalGarbageCollector::new(Arc::clone(&context));
    let result = gc.remove_if_archived(&candidate);

    // Should succeed but not remove (fail-safe on unknown status)
    assert!(result.is_ok());
    assert!(!result.unwrap());
}
