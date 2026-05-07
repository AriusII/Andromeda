//! WAL Segment Garbage Collection with Archive Verification
//!
//! This module implements safe removal of obsolete WAL segments after backup integration.
//!
//! # Overview
//!
//! WAL segments become candidates for garbage collection when:
//! 1. All transactions in the segment are committed (visible)
//! 2. All visible transactions are older than the minimum active snapshot LSN
//! 3. The segment has been archived in at least one backup
//! 4. The segment LSN range does not overlap with recovery requirements
//!
//! # Safety Guarantees
//!
//! - **No loss of durability**: Segments are only removed after successful archive verification
//! - **Recovery safety**: Manifest boundary is respected; no segment required by recovery is removed
//! - **Atomic visibility**: Archive status is verified immediately before removal
//! - **Fail-safe**: If archive verification fails, the segment remains in HotStore
//!
//! # Architecture
//!
//! - `WalGcCandidate`: Identification of a recyclable segment
//! - `identify_gc_candidates()`: Determines candidates based on LSN thresholds
//! - `verify_archived()`: Confirms archive coverage before removal
//! - `safe_remove_segment()`: Deletes from HotStore with rollback capability
//! - `WalGcScheduler`: Periodic background task with metrics
//!
//! # Invariants
//!
//! - A segment can only be GC'd if sealing_lsn < min_active_snapshot_lsn
//! - A segment must be verified archived before removal
//! - A segment cannot be GC'd if creation_lsn <= manifest.required_wal_start_lsn
//! - GC operations never panic; all errors are observable
//! - Zero unsafe code

mod collector;
mod context;
mod model;
mod scheduler;

pub use collector::WalGarbageCollector;
pub use context::WalGcContext;
pub use model::{ArchiveStatus, WalGcAuditEvent, WalGcCandidate, WalGcSummary};
pub use scheduler::{WalGcScheduler, WalGcSchedulerConfig};

#[cfg(test)]
mod tests;
