//! Comprehensive WAL GC Policy Integration Tests
//!
//! Tests cover all four durability boundaries and their interactions:
//! 1. Crash recovery (required_wal_start_lsn from manifest)
//! 2. Active snapshot visibility (min_active_snapshot_lsn from snapshot registry)
//! 3. Standby replication (min_standby_received_lsn from HA quorum)
//! 4. PITR retention (pitr_retention_lsn from backup policy)
//!
//! Test matrix:
//! - N1: Segments reclaimed after backup and snapshot advancement
//! - N2: Segments retained for standby replication (slow replicas)
//! - N3: PITR retention window respected
//! - N4: Recovery boundary never violated
//! - N5: Visibility boundary never violated (active snapshots)
//! - N6: Concurrent GC safety (documented behavior)
//! - N7: Multiple boundary overlaps (edge cases)
//! - N8: GC scheduler with all four boundaries

#[cfg(test)]
mod wal_gc_four_boundaries_tests {
    use andromeda_core::AndromedaResult;
    use andromeda_storage::{Lsn, write_ahead_log::*};
    use std::sync::{Arc, Mutex};

    // ============================================================
    // Test 1: Segments Reclaimed After Backup
    // ============================================================

    #[test]
    fn test_segment_reclaimed_after_backup_and_snapshot_advancement() {
        // Scenario:
        // - Segment [101, 200] is old and archived
        // - Snapshot has advanced past 200
        // - Recovery boundary is at 50
        // - Standby received up to 300
        // - PITR window extends to 400
        // Expected: Segment is reclaimable

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(50),  // recovery
            Lsn::new(250), // snapshot (advanced)
            Lsn::new(300), // standby
            Lsn::new(400), // pitr
        )
        .unwrap();

        let segment = WalGcCandidate::new(1, Lsn::new(101), Lsn::new(200), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        assert_eq!(decision, ReclaimabilityDecision::Reclaimable);
        assert!(decision.is_reclaimable());
    }

    // ============================================================
    // Test 2: Segments Retained for Standby Replication
    // ============================================================

    #[test]
    fn test_segment_retained_until_standby_receives() {
        // Scenario:
        // - Segment [201, 300] is newer than recovery boundary
        // - Snapshot has advanced to 350, so visibility is OK
        // - Standby has ONLY received up to 200 (lagging)
        // - PITR window extends to 400
        // Expected: Segment is BLOCKED by replication (standby hasn't received it yet)

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(50),  // recovery
            Lsn::new(350), // snapshot (advanced, so visibility OK)
            Lsn::new(200), // standby (LAGGING, hasn't received up to 300)
            Lsn::new(400), // pitr
        )
        .unwrap();

        let segment = WalGcCandidate::new(1, Lsn::new(201), Lsn::new(300), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        assert!(matches!(
            decision,
            ReclaimabilityDecision::BlockedByReplication { .. }
        ));
        assert!(!decision.is_reclaimable());
    }

    // ============================================================
    // Test 3: PITR Retention Window Respected
    // ============================================================

    #[test]
    fn test_segment_retained_within_pitr_window() {
        // Scenario:
        // - Segment [251, 350] is old enough for recovery (recovery at 50)
        // - Snapshot has advanced past it (at 400)
        // - Standby received it (at 400)
        // - But PITR window only extends to 300, so segment is within window
        // Expected: Segment is BLOCKED by PITR retention

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(50),  // recovery
            Lsn::new(400), // snapshot (advanced)
            Lsn::new(400), // standby (received)
            Lsn::new(300), // pitr (window ends at 300)
        )
        .unwrap();

        let segment = WalGcCandidate::new(1, Lsn::new(251), Lsn::new(350), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        assert!(matches!(
            decision,
            ReclaimabilityDecision::BlockedByPitrRetention { .. }
        ));
        assert!(!decision.is_reclaimable());
    }

    // ============================================================
    // Test 4: Recovery Boundary Never Violated
    // ============================================================

    #[test]
    fn test_segment_at_recovery_boundary_never_reclaimed() {
        // Scenario:
        // - Segment [100, 200] exactly at recovery boundary
        // - Snapshot is far advanced (at 500)
        // - Standby is caught up (at 500)
        // - PITR window is large (at 600)
        // Expected: Segment is BLOCKED by recovery (even if all else is OK)

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(100), // recovery (segment starts here)
            Lsn::new(500), // snapshot
            Lsn::new(500), // standby
            Lsn::new(600), // pitr
        )
        .unwrap();

        let segment = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        assert!(matches!(
            decision,
            ReclaimabilityDecision::BlockedByRecovery { .. }
        ));
    }

    // ============================================================
    // Test 5: Active Snapshot Visibility Never Violated
    // ============================================================

    #[test]
    fn test_segment_with_active_snapshot_never_reclaimed() {
        // Scenario:
        // - Segment [101, 250] is old (recovery at 50)
        // - But snapshot is only at 250 (segment contains snapshot boundary)
        // - Standby is caught up (at 300)
        // - PITR window is large (at 400)
        // Expected: Segment is BLOCKED by visibility

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(50),  // recovery
            Lsn::new(250), // snapshot (at segment end)
            Lsn::new(300), // standby
            Lsn::new(400), // pitr
        )
        .unwrap();

        let segment = WalGcCandidate::new(1, Lsn::new(101), Lsn::new(250), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        assert!(matches!(
            decision,
            ReclaimabilityDecision::BlockedByVisibility { .. }
        ));
    }

    // ============================================================
    // Test 6: Concurrent GC Safety (Documented Behavior)
    // ============================================================

    #[test]
    fn test_concurrent_policy_queries_return_consistent_results() {
        // Scenario:
        // - Multiple threads query the same policy simultaneously
        // - Expected: Each query returns consistent decision for same segment
        //   (Note: Full concurrency with boundary updates is Wave 14 scope)

        let policy = Arc::new(
            DefaultReclaimabilityPolicy::with_lsns(
                Lsn::new(100),
                Lsn::new(200),
                Lsn::new(300),
                Lsn::new(400),
            )
            .unwrap(),
        );

        let candidate = WalGcCandidate::new(1, Lsn::new(101), Lsn::new(150), 65536).unwrap();

        // Simulate concurrent reads
        let mut handles = vec![];
        for _ in 0..10 {
            let policy_clone = Arc::clone(&policy);
            let candidate = candidate.clone();
            let handle = std::thread::spawn(move || policy_clone.can_reclaim_segment(&candidate));
            handles.push(handle);
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All results should be identical (same policy, same candidate)
        for result in &results[1..] {
            assert_eq!(result, &results[0]);
        }
    }

    // ============================================================
    // Test 7: Multiple Boundary Overlaps (Edge Cases)
    // ============================================================

    #[test]
    fn test_all_four_boundaries_blocking_same_segment() {
        // Scenario: Segment violates all four boundaries simultaneously
        // This shouldn't happen in practice but we test deterministic ordering

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(300), // recovery boundary
            Lsn::new(400), // snapshot boundary
            Lsn::new(350), // standby boundary
            Lsn::new(500), // pitr boundary
        )
        .unwrap();

        // Segment [200, 600] overlaps all boundaries
        let segment = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(600), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        // Recovery check comes first in implementation
        assert!(matches!(
            decision,
            ReclaimabilityDecision::BlockedByRecovery { .. }
        ));
    }

    #[test]
    fn test_segment_just_outside_recovery_but_inside_visibility() {
        // Scenario:
        // - Segment [301, 350] is just after recovery boundary (50)
        // - But snapshot is at 300, so segment overlaps
        // Expected: BLOCKED by visibility (recovery check passes first)

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(300), // recovery
            Lsn::new(300), // snapshot
            Lsn::new(400), // standby
            Lsn::new(500), // pitr
        )
        .unwrap();

        let segment = WalGcCandidate::new(1, Lsn::new(301), Lsn::new(350), 65536).unwrap();

        let decision = policy.can_reclaim_segment(&segment);
        assert!(matches!(
            decision,
            ReclaimabilityDecision::BlockedByVisibility { .. }
        ));
    }

    // ============================================================
    // Test 8: GC Scheduler Integration with All Boundaries
    // ============================================================

    #[test]
    fn test_gc_scheduler_respects_all_retention_boundaries() {
        // Scenario: Simulate a full GC run with multiple segments
        // and multiple retention boundaries active

        // Create segments at various LSN ranges
        let candidates = vec![
            WalGcCandidate::new(1, Lsn::new(10), Lsn::new(50), 65536).unwrap(), // Too old (recovery)
            WalGcCandidate::new(2, Lsn::new(51), Lsn::new(100), 65536).unwrap(), // OK for recovery
            WalGcCandidate::new(3, Lsn::new(101), Lsn::new(150), 65536).unwrap(), // OK for recovery
            WalGcCandidate::new(4, Lsn::new(151), Lsn::new(200), 65536).unwrap(), // Snapshot boundary
            WalGcCandidate::new(5, Lsn::new(201), Lsn::new(250), 65536).unwrap(), // After snapshot
            WalGcCandidate::new(6, Lsn::new(251), Lsn::new(300), 65536).unwrap(), // PITR window
            WalGcCandidate::new(7, Lsn::new(301), Lsn::new(350), 65536).unwrap(), // Standby lagging
        ];

        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(50),  // recovery
            Lsn::new(200), // snapshot
            Lsn::new(250), // standby (lagging)
            Lsn::new(300), // pitr
        )
        .unwrap();

        // Analyze each candidate
        let mut reclaimable_count = 0;
        let mut blocked_recovery = 0;
        let mut blocked_visibility = 0;
        let mut blocked_replication = 0;
        let mut blocked_pitr = 0;

        for candidate in &candidates {
            match policy.can_reclaim_segment(candidate) {
                ReclaimabilityDecision::Reclaimable => reclaimable_count += 1,
                ReclaimabilityDecision::BlockedByRecovery { .. } => blocked_recovery += 1,
                ReclaimabilityDecision::BlockedByVisibility { .. } => blocked_visibility += 1,
                ReclaimabilityDecision::BlockedByReplication { .. } => blocked_replication += 1,
                ReclaimabilityDecision::BlockedByPitrRetention { .. } => blocked_pitr += 1,
            }
        }

        // Verify expected blocking patterns
        assert_eq!(blocked_recovery, 1); // Segment 1
        assert_eq!(reclaimable_count, 2); // Segments 2, 3
        assert_eq!(blocked_visibility, 1); // Segment 4
        assert_eq!(blocked_replication, 1); // Segment 7
        assert_eq!(blocked_pitr, 1); // Segment 6
    }

    // ============================================================
    // Test 9: Boundary Policy Validation
    // ============================================================

    #[test]
    fn test_retention_boundary_policy_invariant_validation() {
        // Valid policy should pass validation
        let valid_policy = RetentionBoundaryPolicy::new(
            Lsn::new(100),
            Lsn::new(200),
            Lsn::new(150),
            Lsn::new(300),
        )
        .unwrap();
        assert!(valid_policy.validate().is_ok());

        // Check ordering constraints
        assert_eq!(valid_policy.gc_boundary_lsn(), Lsn::new(150)); // min of all four
    }

    // ============================================================
    // Test 10: Reclaimability Decision Display and Diagnostics
    // ============================================================

    #[test]
    fn test_reclaimability_decision_provides_diagnostic_info() {
        let blocked_recovery = ReclaimabilityDecision::BlockedByRecovery {
            segment_start_lsn: Lsn::new(100),
            required_recovery_lsn: Lsn::new(200),
        };

        // Check display implementation
        let display_str = format!("{}", blocked_recovery);
        assert!(display_str.contains("recovery"));

        // Check blocking LSN extraction
        assert_eq!(blocked_recovery.blocking_lsn(), Some(Lsn::new(200)));

        // Check that reclaimable returns None
        assert_eq!(ReclaimabilityDecision::Reclaimable.blocking_lsn(), None);
    }
}
