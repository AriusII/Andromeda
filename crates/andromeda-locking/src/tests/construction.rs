use super::*;

#[test]
fn resource_constructors_reject_zero_components() {
    assert!(LockResource::schema(0).is_err());
    assert!(LockResource::object(0, 1).is_err());
    assert!(LockResource::object(1, 0).is_err());
    assert!(LockResource::table(1, 0).is_err());
    assert!(LockResource::page(1, 1, 0).is_err());
    assert!(LockResource::row(1, 1, 0).is_err());
}

#[test]
fn lock_types_construct_with_valid_inputs() {
    let tx_id = TransactionId::new(1);
    let holder = LockHolder::new(tx_id, LockMode::Shared).unwrap();
    let waiter = LockWaiter::new(tx_id, LockMode::Exclusive, 7).unwrap();

    assert_eq!(holder.tx_id, tx_id);
    assert_eq!(holder.mode, LockMode::Shared);
    assert_eq!(waiter.sequence, 7);
}
