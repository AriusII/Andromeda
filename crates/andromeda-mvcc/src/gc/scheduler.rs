//! Garbage Collection Scheduler
//!
//! Provides a bounded, cancel-safe background task that periodically runs MVCC
//! garbage collection evidence collection.
//!
//! The scheduler deliberately does **not** own storage scanning, WAL, commit,
//! or rollback critical paths. Each tick performs at most one synchronous
//! `MvccGarbageCollector::run_gc` call, which currently records a GC pass and
//! returns visibility evidence. Actual version reclamation remains delegated to
//! the storage/coordinator layer.

use std::time::Duration;

mod counters;
mod handle;
mod task;
mod types;

pub use handle::GcSchedulerHandle;
pub use task::GcSchedulerTask;
pub use types::{GcSchedulerExit, GcSchedulerExitReason, GcSchedulerStats};

/// Minimum allowed scheduler interval.
///
/// This prevents zero-duration busy loops while preserving the historical
/// [`GcSchedulerTask::new`] constructor. Callers that need strict validation
/// should use [`GcSchedulerTask::try_new`].
pub const MIN_GC_SCHEDULER_INTERVAL: Duration = Duration::from_millis(1);

#[cfg(test)]
mod tests;
