//! F5 Physical Backup Execution Plan — durable artifact orchestration.
//!
//! This module defines the execution model for copying a cold snapshot + WAL archive
//! to backup storage. All data sources are durable artifacts only (no RAM shortcuts).
//!
//! # Execution Model
//!
//! 1. **Validate manifest** — snapshot identity, WAL range, CRCs
//! 2. **Build extent copy plan** — cold snapshot extents (ColdStore + HotStore)
//! 3. **Build WAL segment copy plan** — sequential WAL segments forming [wal_start_lsn, wal_end_lsn]
//! 4. **Validate parallelization** — disjoint extent ranges can copy in parallel; WAL is sequential
//! 5. **Validate resource limits** — total bytes and segment count within guardrails
//! 6. **Check WAL continuity** — no gaps, no missing segments, checksums valid
//! 7. **Execute copy** — (deferred to V1 physical layer; plan only)
//! 8. **Validate backup copy** — optional checksum validation (resource budget dependent)
//! 9. **Write backup metadata** — snapshot_id, wal_start/end_lsn, copy_timestamp, metadata checksum
//!
//! # Failure Modes
//!
//! - **Incomplete copy**: Metadata checksum is absent or fails → backup is invalid
//! - **WAL gaps**: Missing segment in range → backup is invalid, restore will fail
//! - **Snapshot modification during copy**: Manifest checksum detects this
//! - **Destination storage full**: Backup fails cleanly, source unchanged
//! - **Corrupted WAL segment**: Artifact digest checksum fails validation
//! - **Resource exhaustion**: Copy plan rejects if resource limits exceeded

mod limits;
mod plan;
mod tasks;

pub use limits::BackupResourceLimits;
pub use plan::BackupExecutionPlan;
pub use tasks::{ExtentCopyTask, WalSegmentCopyTask};

#[cfg(test)]
mod tests;
