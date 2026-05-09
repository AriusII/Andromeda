use super::*;

pub(super) fn table_resource() -> LockResource {
    LockResource::table(1, 2).unwrap()
}

pub(super) fn holder(tx_id: u64, mode: LockMode) -> LockHolder {
    LockHolder {
        tx_id: TransactionId::new(tx_id),
        mode,
    }
}

pub(super) fn grant(manager: &LockManager, tx_id: u64, resource: LockResource, mode: LockMode) {
    assert_eq!(
        manager
            .acquire(TransactionId::new(tx_id), resource, mode)
            .unwrap(),
        LockAcquireStatus::Granted
    );
}
