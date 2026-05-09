//! Storage recovery placement/publication contract suite.
//!
//! Pure WAL scan and recovery planning checks have moved to
//! `andromeda-recovery`. Storage keeps only the I/O placement and cold
//! publication integration around recovery workload classification.

#[path = "wal_scan_recovery_contract/placement_and_publication.rs"]
mod placement_and_publication;
#[path = "wal_scan_recovery_contract/support.rs"]
mod support;
