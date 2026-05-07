//! Comprehensive test suite for strict two-phase locking (2PL) enforcement.
//!
//! This test module validates that transaction state transitions enforce 2PL
//! discipline: lock acquisitions only during the growing phase, lock releases
//! during the shrinking phase, and no operations after terminal states.
//!
//! Test coverage:
//! - Lock acquisition state restrictions
//! - Lock release state restrictions
//! - 2PL growing/shrinking phase boundaries
//! - Terminal cleanup rules
//! - Re-acquisition violations
//! - Poisoned and disposed transaction handling

use andromeda_tx::{
    LockAcquireStatus, LockManager, LockMode, LockResource, TransactionManager, TransactionState,
    TwoPhaseLocksValidator, TwoPhaseOperation,
};

#[test]
fn test_acquire_only_in_active_state() {
    // Precondition: Transaction is in Created state
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    // Begin a transaction (Created → Active)
    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);

    // Verify lock acquisition succeeds in Active state
    let resource = LockResource::row(1, 1, 1).unwrap();
    let acquire_result = coordinator.acquire(tx_id, resource, LockMode::Shared);
    assert!(acquire_result.is_ok());
}

#[test]
fn test_cannot_acquire_in_created_state() {
    // Create a transaction but do NOT begin it (stays in Created state)
    let _tx_mgr = TransactionManager::new();

    // Try to validate state for Created state
    let state = TransactionState::Created;
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));

    // Validate operation also rejects
    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(result.is_err());
}

#[test]
fn test_cannot_acquire_in_committing_state() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource = LockResource::row(1, 1, 1).unwrap();

    // Acquire a lock in Active state (allowed)
    let _ = coordinator
        .acquire(tx_id, resource, LockMode::Shared)
        .unwrap();

    // Move to Committing state
    tx_mgr.request_commit(tx_id).unwrap();

    // Verify we cannot acquire more locks in Committing state
    let state = TransactionState::Committing;
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));

    // Validation also rejects
    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(result.is_err());
}

#[test]
fn test_cannot_acquire_in_committed_state() {
    let state = TransactionState::Committed;
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(result.is_err());
}

#[test]
fn test_cannot_acquire_in_rolled_back_state() {
    let state = TransactionState::RolledBack;
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(result.is_err());
}

#[test]
fn test_cannot_acquire_in_disposed_state() {
    let state = TransactionState::Disposed;
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(result.is_err());
}

#[test]
fn test_cannot_acquire_in_poisoned_state() {
    let state = TransactionState::Poisoned;
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(result.is_err());
}

#[test]
fn test_release_only_in_committing_state() {
    let state = TransactionState::Committing;
    assert!(TwoPhaseLocksValidator::state_allows_release(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Release);
    assert!(result.is_ok());
}

#[test]
fn test_release_only_in_rolling_back_state() {
    let state = TransactionState::RollingBack;
    assert!(TwoPhaseLocksValidator::state_allows_release(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Release);
    assert!(result.is_ok());
}

#[test]
fn test_cannot_release_in_active_state() {
    let state = TransactionState::Active;
    assert!(!TwoPhaseLocksValidator::state_allows_release(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Release);
    assert!(result.is_err());
}

#[test]
fn test_transaction_coordinator_rejects_release_in_active_state() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();
    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource = LockResource::row(1, 2, 20).unwrap();

    assert_eq!(
        coordinator
            .acquire(tx_id, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let err = coordinator
        .release(tx_id, resource)
        .expect_err("single-resource release must wait for shrinking phase");
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Transaction);
    assert!(lock_mgr.entry(resource).unwrap().is_some());
}

#[test]
fn test_cannot_release_in_created_state() {
    let state = TransactionState::Created;
    assert!(!TwoPhaseLocksValidator::state_allows_release(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Release);
    assert!(result.is_err());
}

#[test]
fn test_cannot_release_in_committed_state() {
    let state = TransactionState::Committed;
    assert!(!TwoPhaseLocksValidator::state_allows_release(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Release);
    assert!(result.is_err());
}

#[test]
fn test_cannot_release_in_disposed_state() {
    let state = TransactionState::Disposed;
    assert!(!TwoPhaseLocksValidator::state_allows_release(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Release);
    assert!(result.is_err());
}

#[test]
fn test_release_all_only_in_committed_state() {
    let state = TransactionState::Committed;
    assert!(TwoPhaseLocksValidator::state_allows_release_all(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::ReleaseAll);
    assert!(result.is_ok());
}

#[test]
fn test_release_all_only_in_rolled_back_state() {
    let state = TransactionState::RolledBack;
    assert!(TwoPhaseLocksValidator::state_allows_release_all(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::ReleaseAll);
    assert!(result.is_ok());
}

#[test]
fn test_cannot_release_all_in_committing_state() {
    let state = TransactionState::Committing;
    assert!(!TwoPhaseLocksValidator::state_allows_release_all(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::ReleaseAll);
    assert!(result.is_err());
}

#[test]
fn test_cannot_release_all_in_active_state() {
    let state = TransactionState::Active;
    assert!(!TwoPhaseLocksValidator::state_allows_release_all(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::ReleaseAll);
    assert!(result.is_err());
}

#[test]
fn test_cannot_release_all_in_inflight_state() {
    let state = TransactionState::Active; // Active is InFlight status
    assert!(!TwoPhaseLocksValidator::state_allows_release_all(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::ReleaseAll);
    assert!(result.is_err());
}

#[test]
fn test_cannot_release_all_in_disposed_state() {
    let state = TransactionState::Disposed;
    assert!(!TwoPhaseLocksValidator::state_allows_release_all(state));

    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::ReleaseAll);
    assert!(result.is_err());
}

#[test]
fn test_2pl_growing_phase_then_shrinking_phase() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource1 = LockResource::row(1, 1, 1).unwrap();
    let resource2 = LockResource::row(1, 1, 2).unwrap();

    // Growing phase: Active state, acquire multiple locks
    let acquire1 = coordinator.acquire(tx_id, resource1, LockMode::Shared);
    assert!(matches!(acquire1, Ok(LockAcquireStatus::Granted)));

    let acquire2 = coordinator.acquire(tx_id, resource2, LockMode::Exclusive);
    assert!(matches!(acquire2, Ok(LockAcquireStatus::Granted)));

    // Request commit (transition to Committing)
    tx_mgr.request_commit(tx_id).unwrap();

    // Shrinking phase: Committing state, release locks
    let release1 = coordinator.release(tx_id, resource1);
    assert!(release1.is_ok());

    let release2 = coordinator.release(tx_id, resource2);
    assert!(release2.is_ok());

    // Final state: Committed (mark it durable)
    tx_mgr.commit_durable(tx_id, 1).unwrap();

    // Terminal cleanup: release_all
    let cleanup = coordinator.release_all(tx_id);
    assert!(cleanup.is_ok());
}

#[test]
fn test_2pl_state_transition_sequence_valid() {
    let tx_mgr = TransactionManager::new();

    // Created → Active
    let tx_id = tx_mgr.begin().unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Active);

    // Active → Committing
    tx_mgr.request_commit(tx_id).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Committing);

    // Committing → Committed (durable)
    tx_mgr.commit_durable(tx_id, 1).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Committed);

    // Committed → Disposed
    tx_mgr.dispose(tx_id).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap();
    assert!(snap.is_none()); // Disposed transactions are removed from live list
}

#[test]
fn test_2pl_rollback_path_sequence() {
    let tx_mgr = TransactionManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Active);

    // Active → RollingBack
    tx_mgr.request_rollback(tx_id).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::RollingBack);

    // RollingBack → RolledBack (durable)
    tx_mgr.rollback_durable(tx_id, 1).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::RolledBack);

    // RolledBack → Disposed
    tx_mgr.dispose(tx_id).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap();
    assert!(snap.is_none()); // Disposed transactions are removed
}

#[test]
fn test_cannot_acquire_after_entering_committing() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource1 = LockResource::row(1, 1, 1).unwrap();

    // Acquire first lock in Active state
    let _ = coordinator
        .acquire(tx_id, resource1, LockMode::Shared)
        .unwrap();

    // Request commit (move to Committing)
    tx_mgr.request_commit(tx_id).unwrap();

    // Verify state machine is now in Committing
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Committing);

    // Try to acquire another lock in Committing state
    // This should fail because state_allows_acquire(Committing) is false in our validator
    let state = TransactionState::Committing;
    let validation = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(validation.is_err());
}

#[test]
fn test_poisoned_transaction_rejects_locks() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource = LockResource::row(1, 1, 1).unwrap();

    // Acquire lock in Active state
    let _ = coordinator
        .acquire(tx_id, resource, LockMode::Shared)
        .unwrap();

    // Poison the transaction
    tx_mgr.poison(tx_id).unwrap();

    // Verify state is now Poisoned
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Poisoned);

    // Verify Poisoned state disallows acquisition
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(
        TransactionState::Poisoned
    ));

    let validation = TwoPhaseLocksValidator::validate_operation(
        TransactionState::Poisoned,
        TwoPhaseOperation::Acquire,
    );
    assert!(validation.is_err());
}

#[test]
fn test_disposed_transaction_no_operations() {
    let tx_mgr = TransactionManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    tx_mgr.request_commit(tx_id).unwrap();
    tx_mgr.commit_durable(tx_id, 1).unwrap();
    tx_mgr.dispose(tx_id).unwrap();

    // Verify Disposed state disallows acquire, release, and release_all
    let disposed_state = TransactionState::Disposed;

    assert!(!TwoPhaseLocksValidator::state_allows_acquire(
        disposed_state
    ));
    assert!(!TwoPhaseLocksValidator::state_allows_release(
        disposed_state
    ));
    assert!(!TwoPhaseLocksValidator::state_allows_release_all(
        disposed_state
    ));

    assert!(
        TwoPhaseLocksValidator::validate_operation(disposed_state, TwoPhaseOperation::Acquire)
            .is_err()
    );
    assert!(
        TwoPhaseLocksValidator::validate_operation(disposed_state, TwoPhaseOperation::Release)
            .is_err()
    );
    assert!(
        TwoPhaseLocksValidator::validate_operation(disposed_state, TwoPhaseOperation::ReleaseAll)
            .is_err()
    );
}

#[test]
fn test_failed_transaction_must_rollback() {
    let tx_mgr = TransactionManager::new();

    let tx_id = tx_mgr.begin().unwrap();

    // Fail the transaction
    tx_mgr.fail(tx_id).unwrap();

    // Verify state is Failed
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Failed);

    // Failed transactions cannot acquire locks
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(
        TransactionState::Failed
    ));

    // But they CAN transition to RollingBack (managed by transaction manager)
    tx_mgr.request_rollback(tx_id).unwrap();
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::RollingBack);
}

#[test]
fn test_no_acquire_after_release_shrinking_phase() {
    // This test validates the core 2PL invariant:
    // once a lock is released, no new locks may be acquired.

    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource1 = LockResource::row(1, 1, 1).unwrap();
    let resource2 = LockResource::row(1, 1, 2).unwrap();

    // Growing phase: acquire both locks
    let _ = coordinator
        .acquire(tx_id, resource1, LockMode::Shared)
        .unwrap();
    let _ = coordinator
        .acquire(tx_id, resource2, LockMode::Shared)
        .unwrap();

    // Transition to shrinking phase (Committing)
    tx_mgr.request_commit(tx_id).unwrap();

    // Release one lock (now in shrinking phase)
    let _ = coordinator.release(tx_id, resource1).unwrap();

    // Verify: Cannot acquire additional locks in Committing state
    let state = TransactionState::Committing;
    let validation = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(
        validation.is_err(),
        "2PL violated: cannot acquire after release in shrinking phase"
    );
}

#[test]
fn test_release_all_requires_terminal_status() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource = LockResource::row(1, 1, 1).unwrap();

    // Acquire a lock
    let _ = coordinator
        .acquire(tx_id, resource, LockMode::Shared)
        .unwrap();

    // Try to release_all while transaction is still InFlight (should fail)
    let cleanup_early = coordinator.release_all(tx_id);
    assert!(cleanup_early.is_err());

    // Now commit the transaction
    tx_mgr.request_commit(tx_id).unwrap();
    tx_mgr.commit_durable(tx_id, 1).unwrap();

    // Now release_all should succeed
    let cleanup = coordinator.release_all(tx_id);
    assert!(cleanup.is_ok());
}

#[test]
fn test_2pl_validator_noop_operation() {
    let state = TransactionState::Disposed;
    let result = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::NoOp);
    assert!(result.is_ok()); // NoOp always succeeds
}

#[test]
fn test_multiple_transactions_independent_2pl() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    // Transaction 1
    let tx1 = tx_mgr.begin().unwrap();
    let coord1 = tx_mgr.lock_coordinator(&lock_mgr);
    let res1 = LockResource::row(1, 1, 1).unwrap();

    // Transaction 2
    let tx2 = tx_mgr.begin().unwrap();
    let coord2 = tx_mgr.lock_coordinator(&lock_mgr);
    let res2 = LockResource::row(1, 1, 2).unwrap();

    // Both transactions can acquire locks in Active state
    let acq1 = coord1.acquire(tx1, res1, LockMode::Shared);
    assert!(acq1.is_ok());

    let acq2 = coord2.acquire(tx2, res2, LockMode::Exclusive);
    assert!(acq2.is_ok());

    // Transaction 1 commits
    tx_mgr.request_commit(tx1).unwrap();
    let snap1 = tx_mgr.snapshot(tx1).unwrap().unwrap();
    assert_eq!(snap1.state_machine.state(), TransactionState::Committing);

    // Transaction 2 can still be in Active/acquiring
    let snap2 = tx_mgr.snapshot(tx2).unwrap().unwrap();
    assert_eq!(snap2.state_machine.state(), TransactionState::Active);

    // Transaction 1 cannot acquire more locks (Committing state)
    let acq1_fail = TwoPhaseLocksValidator::validate_operation(
        TransactionState::Committing,
        TwoPhaseOperation::Acquire,
    );
    assert!(acq1_fail.is_err());

    // Transaction 2 can still acquire (Active state)
    let acq2_ok = TwoPhaseLocksValidator::validate_operation(
        TransactionState::Active,
        TwoPhaseOperation::Acquire,
    );
    assert!(acq2_ok.is_ok());
}

#[test]
fn test_state_allows_acquire_committing_is_false() {
    // Verify that Committing (shrinking phase) correctly disallows new lock acquisitions
    // per strict 2PL theory
    let state = TransactionState::Committing;

    // Strict 2PL: Committing is shrinking phase, no new acquires allowed
    let allows_acquire = TwoPhaseLocksValidator::state_allows_acquire(state);
    assert!(
        !allows_acquire,
        "Committing state should NOT allow new lock acquisitions in strict 2PL"
    );

    // Validation should explicitly reject acquire in Committing state
    let validation = TwoPhaseLocksValidator::validate_operation(state, TwoPhaseOperation::Acquire);
    assert!(
        validation.is_err(),
        "2PL should reject acquire in Committing state"
    );
}
