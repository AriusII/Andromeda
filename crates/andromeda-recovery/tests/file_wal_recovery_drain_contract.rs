//! File WAL recovery contract suite.
//!
//! The suite is split by recovery evidence type:
//!
//! - transaction visibility and ignored redo records
//! - recoverable truncated/corrupt tails
//! - forensic LSN-chain break handling
//! - recovery report replay/ignore partitioning

#[path = "file_wal_recovery_drain_contract/chain_breaks.rs"]
mod chain_breaks;
#[path = "file_wal_recovery_drain_contract/report_partitioning.rs"]
mod report_partitioning;
#[path = "file_wal_recovery_drain_contract/support.rs"]
mod support;
#[path = "file_wal_recovery_drain_contract/tail_boundaries.rs"]
mod tail_boundaries;
#[path = "file_wal_recovery_drain_contract/transaction_visibility.rs"]
mod transaction_visibility;
