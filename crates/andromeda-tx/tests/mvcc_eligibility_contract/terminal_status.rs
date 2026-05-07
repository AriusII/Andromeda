//! Terminal-status and in-flight coverage. These cases keep rolled-back versions separate from in-flight or unknown transactions.

use super::*;

#[test]
fn test_creator_in_flight_false() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT set committed

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
}
#[test]
fn test_aborted_version_immediately_eligible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 100)
            .unwrap();

    let tx_id = next_tx_id();
    // Mark transaction as RolledBack (aborted)
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Check that is_aborted_version_eligible returns true
    assert!(checker.is_aborted_version_eligible(&version));
}
#[test]
fn test_in_flight_version_not_safe() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT mark as committed or rolled back (remains InFlight)

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_in_flight_version_safe should return false for in-flight txs
    assert!(!checker.is_in_flight_version_safe(&version));
}
#[test]
fn test_aborted_version_bypasses_grace_period() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 1000;
    let checker = VersionEligibilityChecker::new(
        status_table.clone(),
        snapshot_registry.clone(),
        grace_period,
    )
    .unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Mark at gc_epoch=0, check at gc_epoch=1
    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Even though gc_epoch (1) < marked_at_gc_epoch + grace_period (0 + 1000),
    // aborted versions are still eligible
    assert!(checker.is_aborted_version_eligible(&version));
}
#[test]
fn test_aborted_version_bypasses_visibility_check() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Create a snapshot that makes the version appear visible
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // Version with end_ts = 100 would normally be visible
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // But aborted versions don't need visibility check
    assert!(checker.is_aborted_version_eligible(&version));

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_rolled_back_version_immediately_eligible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Rolled back versions should be immediately eligible
    assert!(checker.is_aborted_version_eligible(&version));
}
#[test]
fn test_is_eligible_extended_aborted_path() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 100)
            .unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // is_eligible_extended should immediately return true for aborted txs
    let is_eligible = checker.is_eligible_extended(&version, 1).unwrap();
    assert!(is_eligible);
}
#[test]
fn test_is_eligible_extended_in_flight_path() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Leave in-flight

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_eligible_extended should immediately return false for in-flight txs
    let is_eligible = checker.is_eligible_extended(&version, 10).unwrap();
    assert!(!is_eligible);
}
#[test]
fn test_aborted_version_does_not_count_as_in_flight() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Aborted version is safe to proceed (not in-flight)
    assert!(checker.is_in_flight_version_safe(&version));
}
#[test]
fn test_unknown_tx_defaults_to_in_flight() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Don't record anything in status table

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Unknown txs default to in-flight, so NOT safe
    assert!(!checker.is_in_flight_version_safe(&version));
    assert!(!checker.is_aborted_version_eligible(&version));
}
#[test]
fn test_concurrent_aborted_versions_stress() {
    use std::sync::Arc;
    use std::thread;

    let status_table = Arc::new(TransactionStatusTable::new());
    let snapshot_registry = Arc::new(ActiveSnapshotRegistry::new());
    let checker = Arc::new(
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap(),
    );

    let handles: Vec<_> = (0..50)
        .map(|i| {
            let checker_clone = Arc::clone(&checker);
            let status_table_clone = Arc::clone(&status_table);

            thread::spawn(move || {
                let tx_id = next_tx_id();

                // Half mark as rolled back, half leave in-flight
                if i % 2 == 0 {
                    status_table_clone
                        .record_rolled_back_after_durable_wal(
                            tx_id,
                            andromeda_tx::Lsn::new(1),
                            andromeda_tx::Lsn::new(1),
                        )
                        .unwrap();
                }

                let version = VersionRecord::new(i as u64 + 1, tx_id, 100 + i as u64, 5).unwrap();

                if i % 2 == 0 {
                    // Should be immediately eligible
                    assert!(checker_clone.is_aborted_version_eligible(&version));
                } else {
                    // Should NOT be safe (in-flight)
                    assert!(!checker_clone.is_in_flight_version_safe(&version));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
#[test]
fn test_invariant_no_in_flight_version_reclaimed() {
    // This test validates the secondary invariant:
    // "No in-flight transaction version is ever reclaimed"

    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 1).unwrap();

    let tx_id = next_tx_id();
    // Leave in-flight (do not record status)

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Even with gc_epoch far in future, in-flight versions must not be eligible
    assert!(!checker.is_in_flight_version_safe(&version));

    // Try with extended eligibility check (should fail at fast-path 2)
    let is_eligible = checker.is_eligible_extended(&version, 1_000_000).unwrap();
    assert!(!is_eligible);
}
#[test]
fn test_in_flight_rollback_release_versions() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Initially in-flight
    assert!(!checker.is_in_flight_version_safe(&version));

    // Transaction rolls back
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Now immediately eligible
    assert!(checker.is_aborted_version_eligible(&version));
    assert!(checker.is_in_flight_version_safe(&version));
}
#[test]
fn test_first_failed_criterion_aborted_supersedes_other_failures() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 1000)
            .unwrap();

    let tx_id = next_tx_id();
    // Mark as aborted instead of committed
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Create snapshot to make visibility fail
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // The aborted path bypasses all other checks
    assert!(checker.is_aborted_version_eligible(&version));

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
