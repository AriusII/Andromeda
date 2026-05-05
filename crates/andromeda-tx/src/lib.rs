#![forbid(unsafe_code)]

mod allocator;
pub mod active_snapshot_registry;
pub mod commit_log;
pub mod commit_protocol;
pub mod deadlock_detection;
pub mod gc;
pub mod lock_history;
pub mod lock_manager;
pub mod lock_protocol;
pub mod locking_protocol;
mod manager;
pub mod mvcc;
mod mvcc_snapshot;
mod mvcc_status;
mod mvcc_version;
mod state;
mod trace;
pub mod wal_adapter;

// Re-export MVCC types and functions
pub use mvcc_snapshot::{MvccIsolationPolicy, Snapshot};
pub use mvcc_status::{TransactionStatus, TransactionStatusTable};
pub use mvcc_version::{MvccRowHeader, creator_is_visible, delete_is_visible};

pub use active_snapshot_registry::{
    ActiveSnapshotRegistry, GcError, SnapshotHandle,
};
pub use allocator::TransactionIdAllocator;
pub use commit_log::{CommitLogEntry, CommitLogManager, IsolationLevel};
pub use commit_protocol::CommitProtocol;
pub use deadlock_detection::*;
pub use gc::{GcStatSnapshot, MvccGarbageCollector, GcStats, GcSummary, GcEligibilityChecker, GcSchedulerTask};
pub use gc::mvcc_eligibility::{VersionEligibilityChecker, VersionEligibility, VersionRecord, VersionEligibilityStats};
pub use gc::reclamation::{ReclaimationMark, ReclaimationEligibility, ReclamationCommand};
pub use lock_history::*;
pub use lock_manager::*;
pub use locking_protocol::{TwoPhaseLocksValidator, TwoPhaseOperation};
pub use manager::{TransactionLockCoordinator, TransactionManager, TransactionRecord};
pub use state::*;
pub use trace::*;
pub use wal_adapter::{TxWalAdapterTrait, WalManager, TxWalAdapterError};
