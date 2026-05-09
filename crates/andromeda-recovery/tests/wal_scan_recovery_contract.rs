//! WAL scan and recovery contract suite.
//!
//! Pure WAL scan and recovery planning checks live with recovery/WAL owners.

#[path = "wal_scan_recovery_contract/scan_boundaries.rs"]
mod scan_boundaries;
#[path = "wal_scan_recovery_contract/support.rs"]
mod support;
#[path = "wal_scan_recovery_contract/transaction_replay.rs"]
mod transaction_replay;
