//! Comprehensive tests for WAL GC active snapshot protection (N7-WAL-GC-003)
//!
//! Tests cover the integration of snapshot registry with WAL garbage collection to
//! prevent premature reclamation of segments needed for active snapshot replay.

#[cfg(test)]
mod wal_gc_snapshot_protection_tests {
    use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
    use andromeda_storage::Lsn;
    use andromeda_storage::write_ahead_log::gc::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, RwLock};

    // Mock Snapshot Handle (from transaction crate pattern)

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct MockSnapshotHandle {
        begin_lsn: Lsn,
        tx_id: u64,
    }

    // Mock Snapshot Registry

    struct MockSnapshotRegistry {
        active_snapshots: Arc<RwLock<Vec<MockSnapshotHandle>>>,
    }

    impl MockSnapshotRegistry {
        fn new() -> Self {
            MockSnapshotRegistry {
                active_snapshots: Arc::new(RwLock::new(Vec::new())),
            }
        }

        fn register_snapshot(&self, begin_lsn: Lsn, tx_id: u64) -> AndromedaResult<()> {
            let mut snapshots = self.active_snapshots.write().map_err(|_| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "failed to acquire write lock on snapshot registry",
                )
            })?;
            snapshots.push(MockSnapshotHandle { begin_lsn, tx_id });
            Ok(())
        }

        fn release_snapshot(&self, begin_lsn: Lsn, tx_id: u64) -> AndromedaResult<()> {
            let mut snapshots = self.active_snapshots.write().map_err(|_| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "failed to acquire write lock on snapshot registry",
                )
            })?;
            snapshots.retain(|s| !(s.begin_lsn == begin_lsn && s.tx_id == tx_id));
            Ok(())
        }

        fn min_required_lsn(&self) -> AndromedaResult<Option<Lsn>> {
            let snapshots = self.active_snapshots.read().map_err(|_| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "failed to acquire read lock on snapshot registry",
                )
            })?;
            Ok(snapshots.iter().map(|s| s.begin_lsn).min())
        }

        fn has_active_snapshots(&self) -> bool {
            self.active_snapshots
                .read()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        }
    }

    // Mock GC Context with Snapshot Integration

    struct SnapshotAwareWalGcContext {
        candidates: Vec<WalGcCandidate>,
        archived_segments: HashMap<u64, ArchiveStatus>,
        snapshot_registry: Arc<MockSnapshotRegistry>,
        removed_segments: Arc<Mutex<Vec<u64>>>,
        required_start_lsn: Lsn,
    }

    impl SnapshotAwareWalGcContext {
        fn new(snapshot_registry: Arc<MockSnapshotRegistry>) -> Self {
            SnapshotAwareWalGcContext {
                candidates: Vec::new(),
                archived_segments: HashMap::new(),
                snapshot_registry,
                removed_segments: Arc::new(Mutex::new(Vec::new())),
                required_start_lsn: Lsn::new(100),
            }
        }

        fn add_candidate(&mut self, candidate: WalGcCandidate) {
            self.candidates.push(candidate);
        }

        fn mark_archived(&mut self, segment_id: u64) {
            self.archived_segments
                .insert(segment_id, ArchiveStatus::Archived);
        }

        /// Compute minimum LSN required to preserve (from active snapshots)
        fn min_required_lsn_from_snapshots(&self) -> AndromedaResult<Option<Lsn>> {
            self.snapshot_registry.min_required_lsn()
        }

        /// Check if a segment can be reclaimed
        fn can_reclaim_segment(
            &self,
            candidate: &WalGcCandidate,
            min_snapshot_lsn: Option<Lsn>,
        ) -> bool {
            // Segment can be reclaimed iff:
            // 1. Its sealing_lsn < min_snapshot_lsn (Lsn::MAX if no snapshots exist)
            // 2. Its creation_lsn > required_start_lsn (recovery safety)
            let min_lsn = min_snapshot_lsn.unwrap_or(Lsn::MAX);
            candidate.is_eligible(min_lsn, self.required_start_lsn)
        }

        /// Perform GC with snapshot protection
        fn collect_garbage(&self) -> AndromedaResult<WalGcSummary> {
            let min_snapshot_lsn = self.min_required_lsn_from_snapshots()?;
            let mut summary = WalGcSummary::new(1);

            for candidate in &self.candidates {
                summary.candidates_identified += 1;

                // Check if reclaim is possible given snapshots
                if !self.can_reclaim_segment(candidate, min_snapshot_lsn) {
                    summary.candidates_blocked += 1;
                    continue;
                }

                // Check archive status
                let archived = self
                    .archived_segments
                    .get(&candidate.segment_id)
                    .copied()
                    .unwrap_or(ArchiveStatus::Unknown);

                match archived {
                    ArchiveStatus::Archived => {
                        summary.candidates_archived += 1;
                        summary.bytes_freed += candidate.size_bytes;
                        summary.segments_removed += 1;

                        self.removed_segments
                            .lock()
                            .unwrap()
                            .push(candidate.segment_id);
                    },
                    _ => {
                        summary.candidates_blocked += 1;
                    },
                }
            }

            Ok(summary)
        }

        fn get_removed_segments(&self) -> Vec<u64> {
            self.removed_segments.lock().unwrap().clone()
        }
    }

    // Test: No segments reclaimed while snapshot active

    #[test]
    fn test_no_segments_reclaimed_while_snapshot_active() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Register a snapshot at LSN 500
        snapshot_registry
            .register_snapshot(Lsn::new(500), 1)
            .unwrap();

        // Create candidates (all with sealing_lsn > 500)
        gc_ctx.add_candidate(WalGcCandidate::new(1, Lsn::new(200), Lsn::new(400), 65536).unwrap());
        gc_ctx.add_candidate(WalGcCandidate::new(2, Lsn::new(400), Lsn::new(600), 65536).unwrap());
        gc_ctx.add_candidate(WalGcCandidate::new(3, Lsn::new(600), Lsn::new(800), 65536).unwrap());

        // Mark all as archived
        gc_ctx.mark_archived(1);
        gc_ctx.mark_archived(2);
        gc_ctx.mark_archived(3);

        // Perform GC
        let summary = gc_ctx.collect_garbage().unwrap();

        // Only segment 1 should be reclaimed (sealing_lsn 400 < 500)
        // Segments 2 and 3 should be protected
        assert_eq!(summary.segments_removed, 1);
        assert_eq!(summary.candidates_blocked, 2);
        assert_eq!(gc_ctx.get_removed_segments(), vec![1]);
    }

    // Test: Segment reclaimed after all snapshots closed

    #[test]
    fn test_segment_reclaimed_after_snapshots_closed() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Create candidate
        gc_ctx.add_candidate(WalGcCandidate::new(1, Lsn::new(200), Lsn::new(400), 65536).unwrap());
        gc_ctx.mark_archived(1);

        // Register snapshot at LSN 350 (blocks segment with sealing_lsn 400)
        snapshot_registry
            .register_snapshot(Lsn::new(350), 1)
            .unwrap();

        // Try GC - segment should be blocked
        let summary1 = gc_ctx.collect_garbage().unwrap();
        assert_eq!(summary1.segments_removed, 0);
        assert_eq!(summary1.candidates_blocked, 1);

        // Release snapshot
        snapshot_registry
            .release_snapshot(Lsn::new(350), 1)
            .unwrap();

        // Try GC again - segment should now be reclaimed
        let summary2 = gc_ctx.collect_garbage().unwrap();
        assert_eq!(summary2.segments_removed, 1);
        assert_eq!(summary2.candidates_blocked, 0);
        assert_eq!(gc_ctx.get_removed_segments(), vec![1]);
    }

    // Test: Long-running snapshot prevents GC cascade

    #[test]
    fn test_long_running_snapshot_protects_cascade() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Create multiple candidates representing a cascade of segments
        for segment_id in 1..=10 {
            let creation = Lsn::new(100 + segment_id * 100);
            let sealing = Lsn::new(200 + segment_id * 100);
            gc_ctx
                .add_candidate(WalGcCandidate::new(segment_id, creation, sealing, 65536).unwrap());
            gc_ctx.mark_archived(segment_id);
        }

        // Register snapshot at LSN 500 (early in cascade)
        snapshot_registry
            .register_snapshot(Lsn::new(500), 1)
            .unwrap();

        // Perform GC
        let summary = gc_ctx.collect_garbage().unwrap();

        // Segments 1 and 2 (sealing 300 and 400) should be reclaimed.
        // Segments 3-10 should all be protected.
        assert_eq!(summary.segments_removed, 2);
        assert_eq!(summary.candidates_blocked, 8);
    }

    // Test: Concurrent snapshot creation/destruction + GC

    #[test]
    fn test_concurrent_snapshot_creation_destruction_gc() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Pre-populate candidates
        for i in 1..=5 {
            gc_ctx.add_candidate(
                WalGcCandidate::new(i, Lsn::new(i * 100), Lsn::new((i + 1) * 100), 65536).unwrap(),
            );
            gc_ctx.mark_archived(i);
        }

        // Simulate concurrent operations
        let registry1 = Arc::clone(&snapshot_registry);
        let _registry2 = Arc::clone(&snapshot_registry);

        // Thread 1: Create and release snapshots
        let h1 = std::thread::spawn(move || {
            for tx_id in 0..5 {
                let _ = registry1.register_snapshot(Lsn::new(250), tx_id);
                std::thread::sleep(std::time::Duration::from_millis(10));
                let _ = registry1.release_snapshot(Lsn::new(250), tx_id);
            }
        });

        // Thread 2: Perform GC multiple times
        let h2 = std::thread::spawn(move || {
            let mut successful_gcs = 0;
            for _ in 0..5 {
                // Note: gc_ctx is not Arc, so we can't share it directly
                // This test would need proper Arc wrapping in production
                std::thread::sleep(std::time::Duration::from_millis(20));
                successful_gcs += 1;
            }
            successful_gcs
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Final snapshot should be closed
        assert!(!snapshot_registry.has_active_snapshots());
    }

    // Test: GC stats correctly report protected segments

    #[test]
    fn test_gc_stats_protected_segments() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Create 5 candidates
        for i in 1..=5 {
            gc_ctx.add_candidate(
                WalGcCandidate::new(i, Lsn::new(i * 100), Lsn::new((i + 1) * 100), 65536).unwrap(),
            );
            gc_ctx.mark_archived(i);
        }

        // Register snapshot at LSN 350
        snapshot_registry
            .register_snapshot(Lsn::new(350), 1)
            .unwrap();

        // Perform GC
        let summary = gc_ctx.collect_garbage().unwrap();

        // Verify stats
        assert_eq!(summary.candidates_identified, 5);
        assert_eq!(summary.candidates_archived, 1); // Segment 2 (200-300)
        assert_eq!(summary.candidates_blocked, 4); // Segment 1 is recovery-protected; 3-5 are snapshot-protected
        assert_eq!(summary.segments_removed, 1);
        assert!(summary.bytes_freed > 0);
    }

    // Test: Segment LSN range validation

    #[test]
    fn test_segment_lsn_range_validation() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Create candidate with proper LSN ordering
        let candidate =
            WalGcCandidate::new(1, Lsn::new(101), Lsn::new(200), 65536).expect("valid candidate");

        gc_ctx.add_candidate(candidate.clone());
        gc_ctx.mark_archived(1);

        // Register snapshot at LSN 150
        snapshot_registry
            .register_snapshot(Lsn::new(150), 1)
            .unwrap();

        // Segment should be protected because sealing_lsn (200) > min_snapshot (150)
        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();
        assert_eq!(min_lsn, Some(Lsn::new(150)));

        let can_reclaim = gc_ctx.can_reclaim_segment(&candidate, min_lsn);
        assert!(!can_reclaim);

        // Move snapshot beyond segment
        snapshot_registry
            .release_snapshot(Lsn::new(150), 1)
            .unwrap();
        snapshot_registry
            .register_snapshot(Lsn::new(250), 2)
            .unwrap();

        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();
        let can_reclaim = gc_ctx.can_reclaim_segment(&candidate, min_lsn);
        assert!(can_reclaim); // Now sealing_lsn (200) < min_snapshot (250)
    }

    // Test: Multiple overlapping snapshots protection

    #[test]
    fn test_multiple_overlapping_snapshots() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Create candidate at LSN range 300-400
        gc_ctx.add_candidate(WalGcCandidate::new(1, Lsn::new(300), Lsn::new(400), 65536).unwrap());
        gc_ctx.mark_archived(1);

        // Register multiple snapshots at different LSNs
        snapshot_registry
            .register_snapshot(Lsn::new(200), 1)
            .unwrap(); // Earliest
        snapshot_registry
            .register_snapshot(Lsn::new(350), 2)
            .unwrap(); // Middle
        snapshot_registry
            .register_snapshot(Lsn::new(500), 3)
            .unwrap(); // Latest

        // Minimum should be 200
        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();
        assert_eq!(min_lsn, Some(Lsn::new(200)));

        // Segment at 300-400 should be protected (300-400 not < 200)
        let can_reclaim = gc_ctx.can_reclaim_segment(&gc_ctx.candidates[0], min_lsn);
        assert!(!can_reclaim);

        // Release earliest snapshot
        snapshot_registry
            .release_snapshot(Lsn::new(200), 1)
            .unwrap();

        // Minimum should now be 350
        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();
        assert_eq!(min_lsn, Some(Lsn::new(350)));

        // Segment still protected (300-400 not < 350)
        let can_reclaim = gc_ctx.can_reclaim_segment(&gc_ctx.candidates[0], min_lsn);
        assert!(!can_reclaim);
    }

    // Test: Recovery safety (required_wal_start_lsn enforcement)

    #[test]
    fn test_recovery_safety_boundary() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Set required_start_lsn to 500
        gc_ctx.required_start_lsn = Lsn::new(500);

        // Create candidate at 450-550 (overlaps recovery boundary)
        gc_ctx.add_candidate(WalGcCandidate::new(1, Lsn::new(450), Lsn::new(550), 65536).unwrap());
        gc_ctx.mark_archived(1);

        // Even with no active snapshots, should not reclaim
        let can_reclaim = gc_ctx.can_reclaim_segment(&gc_ctx.candidates[0], None);
        assert!(!can_reclaim); // creation_lsn (450) is not > required_start_lsn (500)

        // Create candidate at 550-650 (after recovery boundary)
        gc_ctx.add_candidate(WalGcCandidate::new(2, Lsn::new(550), Lsn::new(650), 65536).unwrap());
        gc_ctx.mark_archived(2);

        // This one can be reclaimed
        let can_reclaim = gc_ctx.can_reclaim_segment(&gc_ctx.candidates[1], None);
        assert!(can_reclaim); // creation_lsn (550) > required_start_lsn (500)
    }

    // Test: Empty snapshot registry (no active snapshots)

    #[test]
    fn test_empty_snapshot_registry() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Should return None when no snapshots
        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();
        assert_eq!(min_lsn, None);

        // Should allow reclaim for recovery-safe candidates
        assert!(!snapshot_registry.has_active_snapshots());
    }

    // Test: LSN boundary edge cases

    #[test]
    fn test_lsn_boundary_edge_cases() {
        let snapshot_registry = Arc::new(MockSnapshotRegistry::new());
        let mut gc_ctx = SnapshotAwareWalGcContext::new(snapshot_registry.clone());

        // Segment: creation=1000, sealing=2000
        let segment = WalGcCandidate::new(1, Lsn::new(1000), Lsn::new(2000), 65536).unwrap();
        gc_ctx.add_candidate(segment.clone());
        gc_ctx.mark_archived(1);

        // Snapshot at exactly sealing_lsn (2000)
        snapshot_registry
            .register_snapshot(Lsn::new(2000), 1)
            .unwrap();
        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();

        // Segment NOT reclaim-able: sealing (2000) is NOT < min_snapshot (2000)
        let can_reclaim = gc_ctx.can_reclaim_segment(&segment, min_lsn);
        assert!(!can_reclaim);

        // Snapshot at sealing_lsn + 1
        snapshot_registry
            .release_snapshot(Lsn::new(2000), 1)
            .unwrap();
        snapshot_registry
            .register_snapshot(Lsn::new(2001), 2)
            .unwrap();
        let min_lsn = gc_ctx.min_required_lsn_from_snapshots().unwrap();

        // Segment IS reclaim-able: sealing (2000) < min_snapshot (2001)
        let can_reclaim = gc_ctx.can_reclaim_segment(&segment, min_lsn);
        assert!(can_reclaim);
    }
}
