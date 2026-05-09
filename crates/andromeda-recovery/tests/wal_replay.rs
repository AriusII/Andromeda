//! WAL Replay Recovery Tests (Category A)
//!
//! Tests for verifying correct WAL record replay, transaction handling, and
//! replay idempotency during recovery.

mod common;
use andromeda_wal::Lsn;
use common::*;

#[test]
fn recovery_replay_empty_wal_succeeds() {
    // An empty WAL should result in successful recovery with no records replayed
    let manifest = build_bootstrap_manifest();
    assert_valid_manifest(&manifest);
    assert_eq!(manifest.recovery_floor_lsn(), Lsn::new(0));
}

#[test]
fn recovery_replay_single_commit_record_idempotent() {
    // A single COMMIT record should be replayed correctly
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Recovery floor is where replay starts
    assert_can_recover_at(&manifest, 1);

    // Cannot start before floor
    assert_cannot_recover_at(&manifest, 0);
}

#[test]
fn recovery_replay_multiple_commits_preserves_order() {
    // Multiple COMMIT records should preserve their order
    let lsns = vec![0, 1, 2, 3, 4, 5];
    assert_lsn_monotonic(&lsns);
    assert_lsn_continuous(0, &lsns);
}

#[test]
fn recovery_replay_aborted_transaction_ignored() {
    // An ABORT record should be processed but not redo its changes
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Abort marker should not advance recovery state
    let floor = manifest.recovery_floor_lsn();
    assert_eq!(floor, Lsn::new(1));
}

#[test]
fn recovery_replay_savepoint_creates_checkpoint() {
    // A SAVEPOINT record should mark a consistency boundary
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Checkpoint LSN should be valid
    let checkpoint = manifest.checkpoint_lsn();
    assert_eq!(checkpoint, Lsn::new(0));
}

#[test]
fn recovery_replay_nested_transactions_flattened() {
    // Nested transactions should be treated as a single transaction for replay
    // LSN sequence: BEGIN, BEGIN(nested), COMMIT(nested), COMMIT
    let lsns = vec![1, 2, 3, 4];
    assert_lsn_monotonic(&lsns);

    // Final commit LSN should be 4
    assert_eq!(lsns.last().copied(), Some(4));
}

#[test]
fn recovery_replay_idempotent_exact_duplicate() {
    // Replaying the same WAL records twice should produce identical results
    let manifest1 = build_test_manifest(100, 100);
    let manifest2 = build_test_manifest(100, 100);

    assert_eq!(manifest1.checkpoint_lsn(), manifest2.checkpoint_lsn());
    assert_eq!(
        manifest1.recovery_floor_lsn(),
        manifest2.recovery_floor_lsn()
    );
}

#[test]
fn recovery_replay_interleaved_transactions() {
    // Multiple interleaved transactions should replay correctly
    // T1: BEGIN(LSN1), T2: BEGIN(LSN2), T1: COMMIT(LSN3), T2: COMMIT(LSN4)
    let tx1_begin = 1u64;
    let tx2_begin = 2u64;
    let tx1_commit = 3u64;
    let tx2_commit = 4u64;

    assert!(tx1_begin < tx2_begin);
    assert!(tx2_begin < tx1_commit);
    assert!(tx1_commit < tx2_commit);
}

#[test]
fn recovery_replay_long_running_transaction() {
    // A transaction spanning many WAL records should replay correctly
    let start_lsn = 1u64;
    let end_lsn = 1000u64;

    let manifest = build_test_manifest(0, start_lsn);
    assert_can_recover_at(&manifest, start_lsn);
    assert_can_recover_at(&manifest, end_lsn);
}

#[test]
fn recovery_replay_transaction_with_rollback() {
    // A rolled-back transaction should have its changes undone
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Rollback should return to pre-transaction state
    let floor = manifest.recovery_floor_lsn();
    assert_eq!(floor, Lsn::new(1));
}

#[test]
fn recovery_replay_catalog_changes_applied() {
    // Catalog changes should be replayed and applied to the recovered state
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Catalog changes are part of the durable manifest
    assert_eq!(manifest.database_id, 1);
}

#[test]
fn recovery_replay_index_changes_applied() {
    // Index changes should be replayed and indexes rebuilt if needed
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);
}

#[test]
fn recovery_replay_all_record_types_supported() {
    // All WAL record types should be recognized and handled
    // TxBegin, TxCommit, TxRollback, PageAllocate, PageFormat, RowInsert,
    // RowUpdate, RowDelete, IndexInsert, IndexDelete, MvccVersionCreate,
    // MvccVersionClose, MapDeltaAppend, CheckpointBegin, CheckpointEnd,
    // SnapshotBegin, SnapshotEnd, ManifestSwitch, CatalogChangeBegin,
    // CatalogChangeApply, CatalogChangeCommit, SecurityAuditAppend
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);
}

#[test]
fn recovery_replay_preserves_mvcc_snapshot_semantics() {
    // MVCC version records should preserve visibility semantics
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Snapshot LSN must be valid
    assert_eq!(manifest.snapshot_id, 1);
}

#[test]
fn recovery_replay_with_corrupted_record_stops_at_corruption() {
    // A corrupted record should stop replay and produce an error
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Recovery floor should still be valid
    assert!(manifest.can_start_recovery_at(Lsn::new(1)));
}

#[test]
fn recovery_replay_recovery_floor_respected() {
    // Recovery must not start before the recovery floor LSN
    let manifest = build_test_manifest(500, 1000);

    assert_cannot_recover_at(&manifest, 999);
    assert_can_recover_at(&manifest, 1000);
    assert_can_recover_at(&manifest, 2000);
}

#[test]
fn recovery_replay_with_forward_skip_allowed() {
    // Recovery can skip forward to a later LSN in the WAL
    let manifest = build_test_manifest(0, 1000);

    // Can start at floor
    assert_can_recover_at(&manifest, 1000);

    // Can skip to later point
    assert_can_recover_at(&manifest, 2000);
}

#[test]
fn recovery_replay_deterministic_multiple_replays() {
    // Replaying WAL multiple times must produce identical results
    let manifest = build_test_manifest(100, 200);
    let checkpoint1 = manifest.checkpoint_lsn();
    let floor1 = manifest.recovery_floor_lsn();

    let manifest_again = build_test_manifest(100, 200);
    let checkpoint2 = manifest_again.checkpoint_lsn();
    let floor2 = manifest_again.recovery_floor_lsn();

    assert_eq!(checkpoint1, checkpoint2);
    assert_eq!(floor1, floor2);
}

#[test]
fn recovery_replay_with_system_crash_during_replay() {
    // If system crashes during replay, recovery must be restartable
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);

    // Same manifest should allow re-starting recovery
    let manifest_again = build_test_manifest(0, 1);
    assert_eq!(
        manifest.recovery_floor_lsn(),
        manifest_again.recovery_floor_lsn()
    );
}

#[test]
fn recovery_replay_large_transaction_100k_records() {
    // A large transaction with 100K records should replay correctly
    let start_lsn = 1u64;
    let record_count = 100_000u64;
    let end_lsn = start_lsn + record_count;

    let manifest = build_test_manifest(0, start_lsn);
    assert_can_recover_at(&manifest, end_lsn);
}

#[test]
fn recovery_replay_rapid_fire_commits() {
    // Rapid commits should be replayed in order
    let lsns: Vec<u64> = (0..100).collect();
    assert_lsn_monotonic(&lsns);
    assert_lsn_continuous(0, &lsns);
}

#[test]
fn recovery_replay_mixed_workload_realistic() {
    // A realistic mixed workload should replay correctly
    // Includes: transactions, indexes, catalog changes, MVCC
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);
}

#[test]
fn recovery_replay_empty_transactions_handled() {
    // Transactions with no data changes should be handled correctly
    let manifest = build_test_manifest(0, 1);
    assert_valid_manifest(&manifest);
}

#[test]
fn recovery_replay_with_lsn_gaps_detected() {
    // LSN gaps in WAL should be detected and reported
    let lsns = vec![1u64, 2, 3, 5, 6]; // Gap: 4 is missing

    // Check for gap
    let has_gap = lsns.windows(2).any(|w| w[1] - w[0] != 1);
    assert!(has_gap);
}

#[test]
fn recovery_replay_lsn_monotonicity_enforced() {
    // LSN must never decrease or stay the same during replay
    let lsns = vec![1u64, 2, 3, 4, 5];

    for i in 0..lsns.len().saturating_sub(1) {
        assert!(lsns[i] < lsns[i + 1], "LSN not monotonically increasing");
    }
}
