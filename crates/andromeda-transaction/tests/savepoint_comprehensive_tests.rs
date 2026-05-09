//! Comprehensive savepoint test suite.
//!
//! # Scope
//! Covers the full savepoint lifecycle through six major areas:
//!
//! | Task | Area                          | Cases |
//! |------|-------------------------------|-------|
//! |  1   | Lifecycle unit tests          |  35+  |
//! |  2   | MVCC visibility               |  30+  |
//! |  3   | WAL & durability              |  30+  |
//! |  4   | Error handling & rollback     |  25+  |
//! |  5   | Concurrent & isolation        |  20+  |
//! |  6   | Crash recovery & performance  |  15+  |
//!
//! # Invariants tested
//!
//! * `INV-SP-01` – savepoint names are unique within a transaction
//! * `INV-SP-02` – savepoint ids are monotonically increasing per transaction
//! * `INV-SP-03` – rollback_to keeps the target active; discards only descendants
//! * `INV-SP-04` – release discards the target and all descendants
//! * `INV-SP-05` – savepoint operations are only permitted while Active + InFlight
//! * `INV-SP-06` – commit / rollback clears the savepoint stack before state change
//! * `INV-SP-07` – dispose removes live state, status history is retained
//! * `INV-SP-08` – MVCC row visibility is snapshot-timestamp driven, not savepoint driven
//! * `INV-SP-09` – no WAL record is produced by create/release/rollback_to on its own
//! * `INV-SP-10` – WAL replay reconstructs terminal tx status; incomplete txs stay invisible
//! * `INV-SP-11` – savepoint stack is isolated per transaction-id
//! * `INV-SP-12` – failed / poisoned transactions must rollback before disposal

#![allow(clippy::too_many_lines)]

use andromeda_error::AndromedaErrorKind;

use andromeda_mvcc::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};
use andromeda_savepoint::{SavepointId, SavepointRollbackMarker, SavepointStack};
use andromeda_transaction::{CommitLogManager, TransactionManager, TransactionState};
use andromeda_transaction_log::{IsolationLevel, Lsn, TxWalReplayRecord, WalRecordKind};
use andromeda_types::{CatalogVersion, TransactionId};
use std::sync::Arc;

/// Minimal no-op WAL used by every durability test.
struct NoopWal;

#[async_trait::async_trait]
impl andromeda_transaction_log::InvocationWal for NoopWal {
    async fn append(
        &self,
        _kind: WalRecordKind,
        _transaction_id: Option<TransactionId>,
        _payload: &[u8],
    ) -> andromeda_error::AndromedaResult<Lsn> {
        Ok(Lsn::new(1))
    }

    async fn flush_through(&self, lsn: Lsn) -> andromeda_error::AndromedaResult<Lsn> {
        Ok(lsn)
    }
}

fn make_commit_log() -> (CommitLogManager, Arc<TransactionStatusTable>) {
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(Arc::new(NoopWal), status_table.clone());
    (commit_log, status_table)
}

fn ts(v: u64) -> andromeda_time::EngineTimestamp {
    andromeda_time::EngineTimestamp::from_unix_millis(v)
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

#[path = "savepoint_comprehensive_tests/concurrency_isolation.rs"]
mod concurrency_isolation;
#[path = "savepoint_comprehensive_tests/crash_recovery_performance.rs"]
mod crash_recovery_performance;
#[path = "savepoint_comprehensive_tests/error_handling.rs"]
mod error_handling;
#[path = "savepoint_comprehensive_tests/lifecycle.rs"]
mod lifecycle;
#[path = "savepoint_comprehensive_tests/wal_durability.rs"]
mod wal_durability;
