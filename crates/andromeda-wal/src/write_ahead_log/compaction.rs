//! WAL segment compaction with fragmentation detection.
//!
//! This module owns the pure WAL compaction contract: fragmentation metrics,
//! candidate selection, segment rewrite orchestration, and scheduler state.

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
