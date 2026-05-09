#![forbid(unsafe_code)]
#![doc = r#"
Compatibility facade for the extracted Andromeda transaction family.

`andromeda-tx` intentionally remains as the topology-approved migration
boundary for downstream crates that are not yet allowed to depend on the owner
crates directly. It owns no transaction behavior; transaction state, WAL
adapter coordination, MVCC, locking, savepoints, and transaction-log record
shapes live in the focused owner crates re-exported here.
"#]

pub mod active_snapshot_registry {
    pub use andromeda_mvcc::{ActiveSnapshotRegistry, GcError, SnapshotHandle};
}

pub mod commit_log {
    pub use andromeda_transaction::commit_log::CommitLogManager;
    pub use andromeda_transaction_log::{
        CommitLogEntry, InvocationWal, IsolationLevel, RollbackLogEntry, TransactionStatusRebuild,
        TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary, WalRecordKind,
    };
}

pub mod commit_protocol {
    pub use andromeda_transaction::commit_protocol::*;
}

pub mod deadlock_detection {
    pub use andromeda_locking::deadlock_detection::*;
}

pub mod gc {
    pub mod eligibility {
        pub use andromeda_mvcc::gc::eligibility::*;
    }

    pub mod mvcc_eligibility {
        pub use andromeda_mvcc::gc::mvcc_eligibility::*;
    }

    pub mod reclamation {
        pub use andromeda_mvcc::gc::reclamation::*;
    }

    pub mod scheduler {
        pub use andromeda_mvcc::gc::scheduler::*;
    }

    pub use andromeda_mvcc::gc::mvcc_eligibility::{
        VersionEligibility, VersionEligibilityChecker, VersionEligibilityStats, VersionRecord,
    };
    pub use andromeda_mvcc::gc::reclamation::{
        ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationMarkCandidate,
        ReclamationStats,
    };
    pub use andromeda_mvcc::gc::{
        GcEligibilityChecker, GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle,
        GcSchedulerStats, GcSchedulerTask, GcStatSnapshot, GcStats, GcSummary,
        MIN_GC_SCHEDULER_INTERVAL, MvccGarbageCollector,
    };
}

pub mod lock_history {
    pub use andromeda_locking::lock_history::{
        DeadlockAuditTrace, DeadlockDecisionKind, LockPromotionTrace, LockWaitTrace,
    };
    pub use andromeda_transaction::LockReleaseAllTrace;
}

pub mod lock_manager {
    pub use andromeda_locking::*;
}

pub mod locking_protocol {
    pub use andromeda_transaction::{TwoPhaseLocksValidator, TwoPhaseOperation};
}

pub mod mvcc {
    pub use andromeda_mvcc::{
        MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
        creator_is_visible, delete_is_visible,
    };
}

pub mod wal_adapter {
    pub use andromeda_transaction::{
        TxWalAdapterError, TxWalAdapterReplayKind, TxWalAdapterReplayRecord, TxWalAdapterTrait,
        WalManager, append_commit_and_flush, map_tx_wal_replay_records,
    };
}

pub use andromeda_locking::*;
pub use andromeda_mvcc::gc::mvcc_eligibility::{
    VersionEligibility, VersionEligibilityChecker, VersionEligibilityStats, VersionRecord,
};
pub use andromeda_mvcc::gc::reclamation::{
    ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationMarkCandidate,
    ReclamationStats,
};
pub use andromeda_mvcc::gc::{
    GcEligibilityChecker, GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle,
    GcSchedulerStats, GcSchedulerTask, GcStatSnapshot, GcStats, GcSummary,
    MIN_GC_SCHEDULER_INTERVAL, MvccGarbageCollector,
};
pub use andromeda_mvcc::{
    ActiveSnapshotRegistry, GcError, MvccIsolationPolicy, MvccRowHeader, Snapshot, SnapshotHandle,
    TransactionStatus, TransactionStatusTable, creator_is_visible, delete_is_visible,
};
pub use andromeda_savepoint::{
    MAX_WRITE_SET_IMAGE_BYTES, MAX_WRITE_SET_OPERATION_KIND_BYTES, MAX_WRITE_SET_RESOURCE_ID_BYTES,
    Savepoint, SavepointId, SavepointReleaseEvidence, SavepointRollbackEvidence,
    SavepointRollbackMarker, SavepointStack, TxWriteSet, WriteSetEntry, WriteSetImage,
    WriteSetOperationKind, WriteSetOrdinal, WriteSetResourceId,
};
pub use andromeda_transaction::{
    CommitLogManager, CommitProtocol, LockReleaseAllTrace, TransactionEvent,
    TransactionIdAllocator, TransactionLockCoordinator, TransactionManager, TransactionRecord,
    TransactionState, TransactionStateMachine, TransactionTrace, TransactionTransitionCorrelation,
    TwoPhaseLocksValidator, TwoPhaseOperation, TxWalAdapterError, TxWalAdapterReplayKind,
    TxWalAdapterReplayRecord, TxWalAdapterTrait, WalManager, append_commit_and_flush,
    map_tx_wal_replay_records, transaction_phase_code,
};
pub use andromeda_transaction_log::{
    CommitLogEntry, InvocationWal, IsolationLevel, Lsn, RollbackLogEntry, TransactionStatusRebuild,
    TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary, WalRecordKind,
};
