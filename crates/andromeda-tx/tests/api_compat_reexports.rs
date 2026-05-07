#![allow(dead_code, unused_imports)]

//! Compile-only guard for the C5 transaction compatibility facade.
//!
//! Lot 4 splits must keep these root and nested import paths available until
//! downstream crates migrate deliberately. This test must not execute commit,
//! rollback, MVCC visibility, WAL adapter, or lock behavior.

use std::sync::Arc;

use andromeda_tx::commit_log::InvocationWal;
use andromeda_tx::deadlock_detection::{
    DeadlockDecision, DeadlockDecisionOptions, DeadlockDetector, DeadlockPolicy, DeadlockResult,
    WaitForGraph, decide_deadlock_from_lock_manager,
};
use andromeda_tx::mvcc::{
    MvccIsolationPolicy as NestedMvccIsolationPolicy, MvccRowHeader as NestedMvccRowHeader,
    Snapshot as NestedSnapshot, TransactionStatus as NestedTransactionStatus,
    TransactionStatusTable as NestedTransactionStatusTable,
};
use andromeda_tx::wal_adapter::{
    TxWalAdapterReplayKind as NestedTxWalAdapterReplayKind,
    TxWalAdapterReplayRecord as NestedTxWalAdapterReplayRecord,
    TxWalAdapterTrait as NestedTxWalAdapterTrait, WalManager as NestedWalManager,
    append_commit_and_flush as nested_append_commit_and_flush,
    map_tx_wal_replay_records as nested_map_tx_wal_replay_records,
};
use andromeda_tx::{
    ActiveSnapshotRegistry, CommitLogEntry, CommitLogManager, CommitProtocol, GcEligibilityChecker,
    GcSchedulerHandle, GcSchedulerTask, GcStats, IsolationLevel, LockAcquireEvidence,
    LockAcquireStatus, LockManager, LockMode, LockResource, Lsn, MvccGarbageCollector,
    MvccIsolationPolicy, MvccRowHeader, ReclamationEligibility, ReclamationMark,
    ReclamationMarkCandidate, ReclamationStats, RollbackLogEntry, Savepoint, SavepointId,
    SavepointRollbackEvidence, SavepointRollbackMarker, SavepointStack, Snapshot, SnapshotHandle,
    TransactionEvent, TransactionIdAllocator, TransactionLockCoordinator, TransactionManager,
    TransactionRecord, TransactionState, TransactionStateMachine, TransactionStatus,
    TransactionStatusRebuild, TransactionStatusTable, TransactionTrace,
    TransactionTransitionCorrelation, TxWalAdapterError, TxWalAdapterReplayKind,
    TxWalAdapterReplayRecord, TxWalAdapterTrait, TxWalReplayAction, TxWalReplayRecord,
    TxWalReplaySummary, TxWriteSet, WalManager, WalRecordKind, WriteSetEntry, WriteSetImage,
    WriteSetOperationKind, WriteSetResourceId, append_commit_and_flush, creator_is_visible,
    delete_is_visible, map_tx_wal_replay_records, transaction_phase_code,
};

type RootLsn = Lsn;
type RootTransactionState = TransactionState;
type RootSnapshot = Snapshot;
type NestedSnapshotCompat = NestedSnapshot;
type RootCommitEntry = CommitLogEntry;
type RootRollbackEntry = RollbackLogEntry;
type RootReplayRecord = TxWalReplayRecord;
type NestedReplayRecord = NestedTxWalAdapterReplayRecord;

#[test]
fn tx_root_and_nested_facade_imports_compile() {
    fn accepts_invocation_wal(_: Arc<dyn InvocationWal>) {}
    fn accepts_root_wal_manager(_: Arc<dyn WalManager>) {}
    fn accepts_nested_wal_manager(_: Arc<dyn NestedWalManager>) {}
    fn accepts_root_tx_wal_adapter(_: Arc<dyn TxWalAdapterTrait>) {}
    fn accepts_nested_tx_wal_adapter(_: Arc<dyn NestedTxWalAdapterTrait>) {}
    fn accepts_commit(_: Option<RootCommitEntry>) {}
    fn accepts_rollback(_: Option<RootRollbackEntry>) {}

    let _ = accepts_invocation_wal as fn(Arc<dyn InvocationWal>);
    let _ = accepts_root_wal_manager as fn(Arc<dyn WalManager>);
    let _ = accepts_nested_wal_manager as fn(Arc<dyn NestedWalManager>);
    let _ = accepts_root_tx_wal_adapter as fn(Arc<dyn TxWalAdapterTrait>);
    let _ = accepts_nested_tx_wal_adapter as fn(Arc<dyn NestedTxWalAdapterTrait>);
    let _ = accepts_commit as fn(Option<RootCommitEntry>);
    let _ = accepts_rollback as fn(Option<RootRollbackEntry>);
}
