use super::*;
use crate::state::TransactionState;
use andromeda_locking::{LockAcquireStatus, LockManager, LockMode, LockResource};

#[test]
fn begin_allocates_unique_monotonic_ids_and_mirrors_in_flight() {
    let mgr = TransactionManager::new();
    let a = mgr.begin().unwrap();
    let b = mgr.begin().unwrap();
    let c = mgr.begin().unwrap();
    assert!(a.get() < b.get() && b.get() < c.get());
    for id in [a, b, c] {
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
        let snap = mgr.snapshot(id).unwrap().unwrap();
        assert_eq!(snap.state_machine.state(), TransactionState::Active);
        assert_eq!(snap.status, TransactionStatus::InFlight);
        assert_eq!(snap.savepoint_depth, 0);
    }
}

#[test]
fn savepoint_stack_is_transaction_local_and_nested() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();

    let outer = mgr.create_savepoint(id, "outer").unwrap();
    let inner = mgr.create_savepoint(id, "inner").unwrap();
    assert_eq!(outer.id.get(), 1);
    assert_eq!(inner.id.get(), 2);
    assert_eq!(mgr.savepoint_depth(id).unwrap(), 2);
    assert_eq!(mgr.snapshot(id).unwrap().unwrap().savepoint_depth, 2);

    let rollback = mgr.rollback_to_savepoint(id, "outer").unwrap();
    assert_eq!(rollback.target.name.as_str(), "outer");
    assert_eq!(rollback.discarded_descendants.len(), 1);
    assert_eq!(rollback.discarded_descendants[0].name.as_str(), "inner");
    assert_eq!(mgr.savepoint_depth(id).unwrap(), 1);

    let release = mgr.release_savepoint(id, "outer").unwrap();
    assert_eq!(release.released.len(), 1);
    assert_eq!(release.released[0].name.as_str(), "outer");
    assert_eq!(mgr.savepoint_depth(id).unwrap(), 0);
}

#[test]
fn savepoint_names_are_unique_within_transaction_but_not_across_transactions() {
    let mgr = TransactionManager::new();
    let a = mgr.begin().unwrap();
    let b = mgr.begin().unwrap();

    mgr.create_savepoint(a, "sp").unwrap();
    assert_eq!(
        mgr.create_savepoint(a, "sp").unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    mgr.create_savepoint(b, "sp").unwrap();
}

#[test]
fn savepoint_operations_require_active_in_flight_transaction() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.create_savepoint(id, "sp").unwrap();

    mgr.request_commit(id).unwrap();
    assert_eq!(mgr.savepoint_depth(id).unwrap(), 0);
    assert_eq!(
        mgr.create_savepoint(id, "late").unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        mgr.rollback_to_savepoint(id, "sp").unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
}

#[test]
fn full_rollback_clears_savepoints_before_durable_terminal_status() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.create_savepoint(id, "sp").unwrap();

    mgr.request_rollback(id).unwrap();
    assert_eq!(mgr.savepoint_depth(id).unwrap(), 0);
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    mgr.rollback_durable(id, 11).unwrap();
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::RolledBack));
}

#[test]
fn commit_path_requires_durable_lsn_before_status_mirrors_committed() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.request_commit(id).unwrap();

    // Status is still InFlight until durable evidence arrives.
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));

    // A zero LSN is rejected by the underlying state machine.
    let err = mgr.commit_durable(id, 0).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));

    mgr.commit_durable(id, 42).unwrap();
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::Committed));

    let snap = mgr.snapshot(id).unwrap().unwrap();
    assert!(snap.state_machine.is_visible_committed());
    assert_eq!(
        snap.state_machine.durable_commit_lsn(),
        Some(Lsn::new(42))
    );
}

#[test]
fn commit_path_rejects_durable_lsn_behind_commit_record() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.request_commit(id).unwrap();

    let err = mgr
        .commit_durable_after_wal_record(id, 42, 41)
        .expect_err("commit must not become visible before its record is durable");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    let snap = mgr.snapshot(id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Committing);
    assert_eq!(snap.state_machine.durable_commit_lsn(), None);
}

#[test]
fn rollback_path_requires_durable_lsn_before_status_mirrors_rolled_back() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.request_rollback(id).unwrap();
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    mgr.rollback_durable(id, 7).unwrap();
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::RolledBack));
}

#[test]
fn rollback_path_rejects_durable_lsn_behind_rollback_record() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.request_rollback(id).unwrap();

    let err = mgr
        .rollback_durable_after_wal_record(id, 42, 41)
        .expect_err("rollback must not complete before its record is durable");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    let snap = mgr.snapshot(id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::RollingBack);
    assert_eq!(snap.state_machine.durable_rollback_lsn(), None);
}

#[test]
fn single_resource_lock_release_requires_shrinking_phase() {
    let mgr = TransactionManager::new();
    let locks = LockManager::new();
    let id = mgr.begin().unwrap();
    let resource = LockResource::row(1, 77, 770).unwrap();

    assert_eq!(
        mgr.acquire_lock(&locks, id, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let err = mgr
        .release_lock(&locks, id, resource)
        .expect_err("active transactions must not release single lock records");
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert!(locks.entry(resource).unwrap().is_some());

    mgr.request_commit(id).unwrap();
    assert!(mgr.release_lock(&locks, id, resource).unwrap());
    assert_eq!(locks.entry(resource).unwrap(), None);
}

#[test]
fn poison_then_rollback_durable_marks_status_rolled_back() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.poison(id).unwrap();
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    mgr.request_rollback(id).unwrap();
    mgr.rollback_durable(id, 9).unwrap();
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::RolledBack));
}

#[test]
fn unknown_transaction_is_rejected() {
    let mgr = TransactionManager::new();
    let stranger = TransactionId::new(9_999);
    assert_eq!(
        mgr.commit_durable(stranger, 1).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        mgr.request_rollback(stranger).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        mgr.poison(stranger).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
}

#[test]
fn poisoned_manager_mutex_is_reported_as_transaction_error() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();

    let panic_result = std::panic::catch_unwind(|| {
        let _guard = mgr.inner.lock().unwrap();
        panic!("intentional poison for transaction manager mutex test");
    });
    assert!(panic_result.is_err());

    let err = mgr.status(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(err.message(), "transaction manager mutex was poisoned");
}

#[test]
fn dispose_removes_committed_transaction_but_keeps_status_history() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.request_commit(id).unwrap();
    mgr.commit_durable(id, 5).unwrap();
    assert_eq!(mgr.live_count().unwrap(), 1);

    mgr.dispose(id).unwrap();
    assert_eq!(mgr.live_count().unwrap(), 0);
    // Status table preserves the historical outcome for visibility.
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::Committed));
    assert!(mgr.snapshot(id).unwrap().is_none());
}

#[test]
fn recovery_seed_prevents_id_reuse() {
    let mgr = TransactionManager::with_recovered_floor(1_000);
    let id = mgr.begin().unwrap();
    assert!(id.get() > 1_000);

    // Seeding to a lower floor must be rejected.
    let err = mgr.seed_allocator(10).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Seeding to a higher floor lifts the next allocation.
    mgr.seed_allocator(5_000).unwrap();
    let next = mgr.begin().unwrap();
    assert!(next.get() > 5_000);
}

#[test]
fn begin_returns_typed_error_at_id_space_exhaustion() {
    let mgr = TransactionManager::with_recovered_floor(u64::MAX);

    let err = mgr
        .begin()
        .expect_err("begin must fail when the recovered transaction id floor exhausts id space");

    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(mgr.live_count().unwrap(), 0);
    assert_eq!(mgr.allocator().peek_last_issued(), u64::MAX);
}

#[test]
fn cannot_commit_without_request_commit_first() {
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    // Skipping `request_commit` should be rejected by the state machine.
    let err = mgr.commit_durable(id, 1).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
}

#[test]
fn status_does_not_mirror_terminal_state_when_state_machine_rejects() {
    // If durable_commit_lsn is zero, the state machine refuses; status
    // table must remain InFlight (no half-mirrored terminal state).
    let mgr = TransactionManager::new();
    let id = mgr.begin().unwrap();
    mgr.request_commit(id).unwrap();
    assert!(mgr.commit_durable(id, 0).is_err());
    assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
}
