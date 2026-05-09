use std::sync::Arc;

use andromeda_error::AndromedaResult;

use andromeda_types::TransactionId;

use crate::active_snapshot_registry::ActiveSnapshotRegistry;
use crate::status::{TransactionStatus, TransactionStatusTable};

use super::{GcStatSnapshot, GcStats};

/// Summary of a single garbage collection pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcSummary {
    pub versions_scanned: u64,
    pub versions_reclaimed: u64,
    pub min_visible_ts: u64,
}

/// Core MVCC Garbage Collector.
///
/// Determines which row versions are safe to reclaim based on:
/// - Creator transaction status (from `TransactionStatusTable`)
/// - Active snapshot visibility (from `ActiveSnapshotRegistry`)
pub struct MvccGarbageCollector {
    active_snapshot_registry: Arc<ActiveSnapshotRegistry>,
    status_table: Arc<TransactionStatusTable>,
    stats: Arc<GcStats>,
}

impl MvccGarbageCollector {
    /// Create a new garbage collector.
    pub fn new(
        registry: Arc<ActiveSnapshotRegistry>,
        status_table: Arc<TransactionStatusTable>,
    ) -> Self {
        Self {
            active_snapshot_registry: registry,
            status_table,
            stats: Arc::new(GcStats::new()),
        }
    }

    /// Check if a single row version is eligible for reclamation.
    ///
    /// A version is reclaimable if:
    /// 1. Creator transaction is durably committed (per V0 doctrine), AND
    /// 2. end_ts < minimum_visible_timestamp (no active snapshot sees it), AND
    /// 3. end_ts != u64::MAX (version is closed, not live)
    ///
    /// Special case: Rolled-back versions are always reclaimable (no visibility
    /// rule needed; they are never visible to any snapshot).
    pub fn is_version_reclaimable(&self, creator_tx_id: TransactionId, end_ts: u64) -> bool {
        // Step 1: Live versions (end_ts = u64::MAX) are never reclaimed
        if end_ts == u64::MAX {
            return false;
        }

        // Step 2: Get creator transaction status
        let creator_status = self
            .status_table
            .status(creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        // Step 3: Rolled-back versions are always reclaimable
        if matches!(creator_status, TransactionStatus::RolledBack) {
            return true;
        }

        // Step 4: Only committed creators qualify for min_visible_ts check
        if !matches!(creator_status, TransactionStatus::Committed) {
            // InFlight creator: never reclaim (no durable evidence).
            return false;
        }

        // Step 5: Check if end_ts is older than all active snapshots
        let min_visible = self.active_snapshot_registry.minimum_visible_timestamp();
        end_ts < min_visible
    }

    /// Run a single garbage collection pass.
    ///
    /// This is a synchronous operation that computes statistics and returns
    /// a summary. The actual deletion is expected to be done by a higher-level
    /// coordinator (storage layer) that iterates pages.
    ///
    /// Callers should invoke this periodically (e.g., every 100ms) or on demand
    /// after significant snapshot release.
    pub fn run_gc(&self) -> AndromedaResult<GcSummary> {
        let min_visible_ts = self.active_snapshot_registry.minimum_visible_timestamp();
        let now_ms = self.current_time_ms();

        // In a real implementation, this would iterate through all pages
        // in the heap table and check each version. For now, we record the
        // GC run occurred and return a summary with zero scanned/reclaimed.
        //
        // The actual reclamation is delegated to the storage layer
        // (e.g., during page compaction or explicit GC commands).

        self.stats.record_run(now_ms);

        Ok(GcSummary {
            versions_scanned: 0,
            versions_reclaimed: 0,
            min_visible_ts,
        })
    }

    /// Record that `count` versions were scanned during the current GC context.
    ///
    /// This is called by external code (e.g., storage layer) that iterates
    /// versions and wants to update GC statistics.
    pub fn record_versions_scanned(&self, count: u64) {
        self.stats.record_scan(count);
    }

    /// Record that `count` versions were reclaimed during the current GC context.
    ///
    /// This is called by external code after marking versions as deleted.
    pub fn record_versions_reclaimed(&self, count: u64) {
        self.stats.record_reclaim(count);
    }

    /// Get a snapshot of current GC statistics.
    pub fn get_stats(&self) -> GcStatSnapshot {
        GcStatSnapshot {
            versions_scanned: self.stats.versions_scanned(),
            versions_reclaimed: self.stats.versions_reclaimed(),
            runs: self.stats.runs(),
            last_run_ms: self.stats.last_run_ms(),
        }
    }

    /// Get the minimum visible timestamp (threshold for reclamation).
    pub fn minimum_visible_timestamp(&self) -> u64 {
        self.active_snapshot_registry.minimum_visible_timestamp()
    }

    /// Get the count of active snapshots currently tracked by the registry.
    pub fn active_snapshot_count(&self) -> usize {
        self.active_snapshot_registry.active_snapshot_count()
    }

    fn current_time_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
