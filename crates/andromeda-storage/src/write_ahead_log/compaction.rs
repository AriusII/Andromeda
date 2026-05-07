//! WAL Segment Compaction with Fragmentation Detection
//!
//! This module implements safe, efficient compaction of WAL segments that have accumulated
//! significant dead space from garbage collection operations.
//!
//! # Overview
//!
//! WAL segments become fragmented when garbage collection removes records (e.g., GC'd versions,
//! completed undo logs, ancient transactions). The rewritten segment occupies the same physical
//! space as the original, but only contains live records. Compaction reduces storage overhead
//! by rewriting fragmented segments to new temporary segments, filtering out dead records.
//!
//! Compaction is only applied to segments with fragmentation_ratio >= 0.30 (30% dead space).
//!
//! # Safety Guarantees
//!
//! - **LSN Monotonicity**: Rewritten segments maintain strict LSN ordering and previous-LSN chaining
//! - **Visibility Preservation**: Records visible to active snapshots are always retained
//! - **Transaction Integrity**: Uncommitted/undoable transactions are preserved
//! - **Atomic Swap**: Temporary → active is atomic; on failure, old segment remains intact
//! - **No Data Loss**: Failed compaction leaves segment unchanged; temporary segment abandoned
//! - **Recovery Safe**: Manifest boundaries are respected; recovery segments never compacted
//!
//! # Architecture
//!
//! - `FragmentationMetrics`: Identifies compaction candidates (dead_bytes, ratio)
//! - `CompactionContext`: Trait for integration with WAL manager and snapshot registry
//! - `identify_compaction_candidates()`: Find segments exceeding fragmentation threshold
//! - `compact_segment()`: Rewrite segment with dead records filtered out
//! - `WalCompactionScheduler`: Background task with configurable interval and thresholds
//!
//! # Invariants
//!
//! - Fragmentation ratio must be calculated: dead_bytes / total_bytes
//! - Compaction only proceeds if ratio >= 0.30
//! - Records are kept if: LSN in [snapshot_start, snapshot_end] OR marked undoable
//! - Rewritten segment uses fresh segment_id (monotonically increasing from old ID)
//! - Swap is all-or-nothing: either new replaces old, or old remains unchanged
//! - All errors are observable; no panics in compaction path
//! - Zero unsafe code

mod context;
mod model;
mod operations;
mod scheduler;

pub use context::CompactionContext;
pub use model::{
    CompactionResult, FragmentationMetrics, WalCompactionAuditEvent, WalCompactionSummary,
};
pub use operations::{compact_segment, identify_compaction_candidates};
pub use scheduler::{WalCompactionScheduler, WalCompactionSchedulerConfig};

#[cfg(test)]
mod tests;
