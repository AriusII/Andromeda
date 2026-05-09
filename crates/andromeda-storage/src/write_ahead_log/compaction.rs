//! Compatibility reexports for WAL compaction ownership.
//!
//! Canonical compaction types now live in
//! `andromeda_wal::write_ahead_log::compaction`.

pub use andromeda_wal::{
    CompactionContext, CompactionResult, FragmentationMetrics, WalCompactionAuditEvent,
    WalCompactionScheduler, WalCompactionSchedulerConfig, WalCompactionSummary, compact_segment,
    identify_compaction_candidates,
};
