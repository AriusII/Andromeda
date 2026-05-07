//! Integration tests for WAL segment compaction contract compliance.
//!
//! Tests cover fragmentation detection, compaction filtering, scheduler
//! behavior, error recovery, and edge-case result accounting.

#[path = "wal_compaction_contract/compaction.rs"]
mod compaction;
#[path = "wal_compaction_contract/fragmentation.rs"]
mod fragmentation;
#[path = "wal_compaction_contract/scheduler.rs"]
mod scheduler;
#[path = "wal_compaction_contract/support.rs"]
mod support;
