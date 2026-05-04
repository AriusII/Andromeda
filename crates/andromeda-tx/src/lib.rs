#![forbid(unsafe_code)]

mod mvcc_refactored;
mod mvcc_snapshot;
mod mvcc_status;
mod mvcc_version;
mod state;
mod trace;

// Re-export MVCC types and functions
pub use mvcc_snapshot::{MvccIsolationPolicy, Snapshot};
pub use mvcc_status::{TransactionStatus, TransactionStatusTable};
pub use mvcc_version::{creator_is_visible, delete_is_visible, MvccRowHeader};

// Legacy compatibility - re-export under mvcc for compatibility
pub mod mvcc {
    pub use super::mvcc_snapshot::*;
    pub use super::mvcc_status::*;
    pub use super::mvcc_version::*;
}

pub use state::*;
pub use trace::*;
