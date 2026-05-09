use andromeda_error::AndromedaErrorKind;
use andromeda_mvcc::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};
use andromeda_types::{CatalogVersion, TransactionId};

fn durable_status_lsn() -> andromeda_transaction_log::Lsn {
    andromeda_transaction_log::Lsn::new(1)
}

/// Build a minimal RepeatableRead snapshot owned by `tx_id` at `timestamp`.
fn snapshot_rr(
    tx_id: TransactionId,
    timestamp: u64,
    active: impl IntoIterator<Item = TransactionId>,
) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(tx_id),
        active,
    )
    .expect("valid snapshot")
}

/// Build a minimal ReadCommitted snapshot at `timestamp`.
fn snapshot_rc(timestamp: u64) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )
    .expect("valid snapshot")
}

#[path = "savepoint_mvcc_visibility_contract/delete_visibility.rs"]
mod delete_visibility;
#[path = "savepoint_mvcc_visibility_contract/row_validation.rs"]
mod row_validation;
#[path = "savepoint_mvcc_visibility_contract/snapshot_semantics.rs"]
mod snapshot_semantics;
#[path = "savepoint_mvcc_visibility_contract/timestamp_visibility.rs"]
mod timestamp_visibility;
