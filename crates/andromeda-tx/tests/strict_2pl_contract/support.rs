pub(crate) use andromeda_core::{AndromedaErrorKind, TransactionId};
pub(crate) use andromeda_tx::{
    LockAcquireStatus, LockManager, LockMode, LockResource, TransactionManager, TransactionState,
    TwoPhaseLocksValidator, TwoPhaseOperation,
};

pub(crate) fn row_resource(row_id: u64) -> LockResource {
    LockResource::row(1, 1, row_id).unwrap()
}

pub(crate) fn assert_operation_allowed(state: TransactionState, operation: TwoPhaseOperation) {
    assert!(TwoPhaseLocksValidator::validate_operation(state, operation).is_ok());
}

pub(crate) fn assert_operation_rejected(state: TransactionState, operation: TwoPhaseOperation) {
    assert!(TwoPhaseLocksValidator::validate_operation(state, operation).is_err());
}

pub(crate) fn assert_acquire_rejected(state: TransactionState) {
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));
    assert_operation_rejected(state, TwoPhaseOperation::Acquire);
}

pub(crate) fn assert_release_rejected(state: TransactionState) {
    assert!(!TwoPhaseLocksValidator::state_allows_release(state));
    assert_operation_rejected(state, TwoPhaseOperation::Release);
}

pub(crate) fn assert_release_all_rejected(state: TransactionState) {
    assert!(!TwoPhaseLocksValidator::state_allows_release_all(state));
    assert_operation_rejected(state, TwoPhaseOperation::ReleaseAll);
}

pub(crate) fn assert_tx_state(
    tx_mgr: &TransactionManager,
    tx_id: TransactionId,
    expected: TransactionState,
) {
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), expected);
}

pub(crate) fn assert_tx_disposed(tx_mgr: &TransactionManager, tx_id: TransactionId) {
    assert!(tx_mgr.snapshot(tx_id).unwrap().is_none());
}
