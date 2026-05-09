pub(crate) use andromeda_error::AndromedaErrorKind;
pub(crate) use andromeda_locking::{LockAcquireStatus, LockManager, LockMode, LockResource};
pub(crate) use andromeda_transaction::{
    TransactionManager, TransactionState, TwoPhaseLocksValidator, TwoPhaseOperation,
};
pub(crate) use andromeda_types::TransactionId;

pub(crate) fn row_resource(row_id: u64) -> LockResource {
    LockResource::row(1, 1, row_id).unwrap()
}

pub(crate) fn assert_operation_rejected(state: TransactionState, operation: TwoPhaseOperation) {
    assert!(TwoPhaseLocksValidator::validate_operation(state, operation).is_err());
}

pub(crate) fn assert_acquire_rejected(state: TransactionState) {
    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));
    assert_operation_rejected(state, TwoPhaseOperation::Acquire);
}

pub(crate) fn assert_tx_state(
    tx_mgr: &TransactionManager,
    tx_id: TransactionId,
    expected: TransactionState,
) {
    let snap = tx_mgr.snapshot(tx_id).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), expected);
}
