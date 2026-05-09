//! Contract tests for the MVCC version eligibility checker.
//!
//! Coverage is split by decision family so C5-sensitive eligibility contracts stay
//! reviewable: long readers, terminal status, durable WAL status, GC eligibility,
//! and retention-frontier boundaries.

use andromeda_core::TransactionId;
use andromeda_mvcc::gc::mvcc_eligibility::{VersionEligibilityChecker, VersionRecord};
use andromeda_mvcc::{ActiveSnapshotRegistry, SnapshotHandle, TransactionStatusTable};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);

fn next_tx_id() -> TransactionId {
    TransactionId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
}

fn make_status_table() -> Arc<TransactionStatusTable> {
    Arc::new(TransactionStatusTable::new())
}

fn make_snapshot_registry() -> Arc<ActiveSnapshotRegistry> {
    Arc::new(ActiveSnapshotRegistry::new())
}

#[path = "mvcc_eligibility_contract/gc_eligibility.rs"]
mod gc_eligibility;
#[path = "mvcc_eligibility_contract/long_reader.rs"]
mod long_reader;
#[path = "mvcc_eligibility_contract/retention_boundary.rs"]
mod retention_boundary;
#[path = "mvcc_eligibility_contract/terminal_status.rs"]
mod terminal_status;
#[path = "mvcc_eligibility_contract/wal_durability.rs"]
mod wal_durability;
