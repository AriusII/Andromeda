use super::*;

pub(super) fn schema_resource(schema_id: u64) -> LockResource {
    LockResource::schema(schema_id).unwrap()
}

pub(super) fn table_resource(table_id: u64) -> LockResource {
    LockResource::table(1, table_id).unwrap()
}

pub(super) fn row_resource(table_id: u64, row_id: u64) -> LockResource {
    LockResource::row(1, table_id, row_id).unwrap()
}

pub(super) fn holder(tx_id: TransactionId, mode: LockMode) -> LockHolder {
    LockHolder { tx_id, mode }
}

pub(super) fn assert_granted(
    manager: &LockManager,
    tx_id: TransactionId,
    resource: LockResource,
    mode: LockMode,
) {
    assert_eq!(
        manager.acquire(tx_id, resource, mode).unwrap(),
        LockAcquireStatus::Granted
    );
}

pub(super) fn assert_transaction_error<T>(result: AndromedaResult<T>) {
    match result {
        Ok(_) => panic!("expected transaction error"),
        Err(error) => assert_eq!(error.kind(), AndromedaErrorKind::Transaction),
    }
}
