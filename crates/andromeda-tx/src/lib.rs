#![forbid(unsafe_code)]

pub mod active_snapshot_registry;
mod allocator;
pub mod commit_log;
pub mod commit_protocol;
pub mod deadlock_detection;
pub mod gc;
pub mod lock_history;
pub mod lock_manager;
pub mod lock_protocol;
pub mod locking_protocol;
mod lsn;
mod manager;
pub mod mvcc;
mod mvcc_snapshot;
mod mvcc_status;
mod mvcc_version;
mod savepoint;
mod state;
mod trace;
pub mod wal_adapter;

// Re-export MVCC types and functions
pub use mvcc_snapshot::{MvccIsolationPolicy, Snapshot};
pub use mvcc_status::{TransactionStatus, TransactionStatusTable};
pub use mvcc_version::{MvccRowHeader, creator_is_visible, delete_is_visible};

pub use active_snapshot_registry::{ActiveSnapshotRegistry, GcError, SnapshotHandle};
pub use allocator::TransactionIdAllocator;
/// Transaction WAL record kinds exposed for `InvocationWal` implementations.
///
/// This is intentionally part of the transaction crate boundary: transaction tests
/// and storage/adapter crates that implement the public commit-log WAL trait must
/// be able to pattern-match the transaction-local record kind without depending on
/// the storage WAL enum.
pub use commit_log::WalRecordKind;
pub use commit_log::{
    CommitLogEntry, CommitLogManager, IsolationLevel, RollbackLogEntry, TransactionStatusRebuild,
    TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary,
};
pub use commit_protocol::CommitProtocol;
pub use deadlock_detection::*;
pub use gc::mvcc_eligibility::{
    VersionEligibility, VersionEligibilityChecker, VersionEligibilityStats, VersionRecord,
};
pub use gc::reclamation::{
    ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationStats,
};
pub use gc::{
    GcEligibilityChecker, GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle,
    GcSchedulerStats, GcSchedulerTask, GcStatSnapshot, GcStats, GcSummary,
    MIN_GC_SCHEDULER_INTERVAL, MvccGarbageCollector,
};
pub use lock_history::*;
pub use lock_manager::*;
pub use locking_protocol::{TwoPhaseLocksValidator, TwoPhaseOperation};
/// Transaction-local durable log sequence number used at WAL adapter boundaries.
///
/// The transaction crate deliberately owns this boundary value instead of importing
/// the storage crate's `Lsn`; storage implementations convert at the adapter edge.
/// It remains public because `CommitLogEntry`, `InvocationWal`, `WalManager`, and
/// `TxWalAdapterTrait` expose LSNs in their public contracts.
pub use lsn::Lsn;
pub use manager::{TransactionLockCoordinator, TransactionManager, TransactionRecord};
pub use savepoint::{
    Savepoint, SavepointId, SavepointReleaseEvidence, SavepointRollbackEvidence,
    SavepointRollbackMarker, SavepointStack,
};
pub use state::*;
pub use trace::*;
pub use wal_adapter::{TxWalAdapterError, TxWalAdapterTrait, WalManager};
