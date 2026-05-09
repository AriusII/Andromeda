use super::*;

fn durable_status_lsn() -> andromeda_transaction_log::Lsn {
    andromeda_transaction_log::Lsn::new(1)
}

#[path = "mvcc_visibility/delete_visibility.rs"]
mod delete_visibility;
#[path = "mvcc_visibility/row_validation.rs"]
mod row_validation;
#[path = "mvcc_visibility/snapshot_semantics.rs"]
mod snapshot_semantics;
#[path = "mvcc_visibility/timestamp_visibility.rs"]
mod timestamp_visibility;
