//! Compatibility re-exports for transaction-log WAL boundaries.

pub use andromeda_transaction_log::InvocationWal;
pub(super) use andromeda_transaction_log::{encode_commit_payload, encode_rollback_payload};
