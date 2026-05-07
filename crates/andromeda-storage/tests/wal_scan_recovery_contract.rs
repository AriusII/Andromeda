//! WAL scan and recovery contract suite.
//!
//! The suite keeps recovery planning, WAL scan boundary, placement, and
//! cold-publication checks focused while sharing typed WAL and manifest
//! fixtures. Each module states the crash point, durable WAL prefix, expected
//! replay decision, and observable recovery evidence it owns.

#[path = "wal_scan_recovery_contract/placement_and_publication.rs"]
mod placement_and_publication;
#[path = "wal_scan_recovery_contract/scan_boundaries.rs"]
mod scan_boundaries;
#[path = "wal_scan_recovery_contract/support.rs"]
mod support;
#[path = "wal_scan_recovery_contract/transaction_replay.rs"]
mod transaction_replay;
