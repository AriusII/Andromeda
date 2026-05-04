#![forbid(unsafe_code)]

mod allocator;
mod manager;
pub mod mvcc;
mod mvcc_snapshot;
mod mvcc_status;
mod mvcc_version;
mod state;
mod trace;

// Re-export MVCC types and functions
pub use mvcc_snapshot::{MvccIsolationPolicy, Snapshot};
pub use mvcc_status::{TransactionStatus, TransactionStatusTable};
pub use mvcc_version::{creator_is_visible, delete_is_visible, MvccRowHeader};

pub use allocator::TransactionIdAllocator;
pub use manager::{TransactionManager, TransactionRecord};
pub use state::*;
pub use trace::*;
