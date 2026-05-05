//! MVCC Garbage Collection Engine
//!
//! This module implements the core garbage collection logic for MVCC versions.
//! A row version is eligible for reclamation when:
//!

pub mod eligibility;
pub mod mvcc_eligibility;
pub mod reclamation;
pub mod scheduler;

pub use eligibility::GcEligibilityChecker;
pub use mvcc_eligibility::{VersionEligibilityChecker, VersionEligibility, VersionRecord, VersionEligibilityStats};
pub use reclamation::{ReclaimationMark, ReclaimationEligibility, ReclamationCommand};
pub use scheduler::GcSchedulerTask;

//! 1. Its creator transaction has been durably committed (per V0 doctrine), AND
//! 2. Its `end_ts` is strictly less than the minimum visible timestamp (no active
//!    snapshot can see it), AND
//! 3. The version is closed (`end_ts ≠ u64::MAX`)
//!
//! # Thread Safety
//!
//! All GC operations are thread-safe and make no unsafe assumptions.
//! - `MvccGarbageCollector` can be shared via Arc
//! - Stats updates use AtomicU64 with relaxed ordering
//! - Reads from `TransactionStatusTable` and `ActiveSnapshotRegistry` are atomic
//!
//! # Design Decisions
//!
//! - **No separate HeapTable scanning**: GC assumes a higher-level coordinator
//!   provides page iteration and calls `mark_version_reclaimed()` per version.
//! - **Status lookups are conservative**: Only `Committed` status qualifies a creator.
//!   `InFlight` or `RolledBack` versions follow different rules (see logic below).
//! - **Idempotent marking**: Calling `mark_version_reclaimed()` multiple times is safe.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use andromeda_core::{AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::active_snapshot_registry::ActiveSnapshotRegistry;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

/// Statistics for garbage collection runs.
#[derive(Debug, Clone)]
pub struct GcStats {
    versions_scanned: AtomicU64,
    versions_reclaimed: AtomicU64,
    runs: AtomicU64,
    last_run_ms: AtomicU64,  // Wall-clock timestamp (ms) of last GC run
}

impl GcStats {
    pub fn new() -> Self {
        GcStats {
            versions_scanned: AtomicU64::new(0),
            versions_reclaimed: AtomicU64::new(0),
            runs: AtomicU64::new(0),
            last_run_ms: AtomicU64::new(0),
        }
    }

    pub fn versions_scanned(&self) -> u64 {
        self.versions_scanned.load(Ordering::Relaxed)
    }

    pub fn versions_reclaimed(&self) -> u64 {
        self.versions_reclaimed.load(Ordering::Relaxed)
    }

    pub fn runs(&self) -> u64 {
        self.runs.load(Ordering::Relaxed)
    }

    pub fn last_run_ms(&self) -> u64 {
        self.last_run_ms.load(Ordering::Relaxed)
    }

    fn record_scan(&self, count: u64) {
        self.versions_scanned.fetch_add(count, Ordering::Relaxed);
    }

    fn record_reclaim(&self, count: u64) {
        self.versions_reclaimed.fetch_add(count, Ordering::Relaxed);
    }

    fn record_run(&self, now_ms: u64) {
        self.runs.fetch_add(1, Ordering::Relaxed);
        self.last_run_ms.store(now_ms, Ordering::Relaxed);
    }
}

impl Default for GcStats {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// 3. end_ts ≠ u64::MAX (version is closed, not live)
    ///
    /// Special case: Rolled-back versions are always reclaimable (no visibility
    /// rule needed; they are never visible to any snapshot).
    pub fn is_version_reclaimable(
        &self,
        creator_tx_id: TransactionId,
        end_ts: u64,
    ) -> bool {
        // Step 1: Live versions (end_ts = u64::MAX) are never reclaimed
        if end_ts == u64::MAX {
            return false;
        }

        // Step 2: Get creator transaction status
        let creator_status = self.status_table.status(creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        // Step 3: Rolled-back versions are always reclaimable
        if matches!(creator_status, TransactionStatus::RolledBack) {
            return true;
        }

        // Step 4: Only committed creators qualify for min_visible_ts check
        if !matches!(creator_status, TransactionStatus::Committed) {
            // InFlight creator → never reclaim (version is invisible anyway,
            // but we enforce strict durable-commit rule)
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
        #[allow(unsafe_code)]  // Safety: system time call is safe
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Snapshot of GC statistics at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcStatSnapshot {
    pub versions_scanned: u64,
    pub versions_reclaimed: u64,
    pub runs: u64,
    pub last_run_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_snapshot_registry::SnapshotHandle;

    fn make_collector() -> MvccGarbageCollector {
        MvccGarbageCollector::new(
            Arc::new(ActiveSnapshotRegistry::new()),
            Arc::new(TransactionStatusTable::new()),
        )
    }

    #[test]
    fn test_live_version_never_reclaimed() {
        let collector = make_collector();
        let tx_id = TransactionId::new(1);

        // Live version (end_ts = u64::MAX) should never be reclaimed
        assert!(!collector.is_version_reclaimable(tx_id, u64::MAX));
    }

    #[test]
    fn test_in_flight_version_never_reclaimed() {
        let collector = make_collector();
        let tx_id = TransactionId::new(1);

        // InFlight creator → never reclaim (no durable evidence)
        assert!(!collector.is_version_reclaimable(tx_id, 100));
        assert!(!collector.is_version_reclaimable(tx_id, 500));
    }

    #[test]
    fn test_rolled_back_version_always_reclaimed() {
        let status_table = TransactionStatusTable::new();
        let mut status_table_mut = status_table.clone();
        let tx_id = TransactionId::new(1);

        // Record rollback
        status_table_mut
            .record(tx_id, TransactionStatus::RolledBack)
            .expect("record rollback");

        let collector = MvccGarbageCollector::new(
            Arc::new(ActiveSnapshotRegistry::new()),
            Arc::new(status_table_mut),
        );

        // Rolled-back versions are always reclaimable
        assert!(collector.is_version_reclaimable(tx_id, 1));
        assert!(collector.is_version_reclaimable(tx_id, 100));
        assert!(collector.is_version_reclaimable(tx_id, 1000));
    }

    #[test]
    fn test_committed_version_older_than_min_visible() {
        let mut status_table = TransactionStatusTable::new();
        let registry = ActiveSnapshotRegistry::new();
        let tx_id = TransactionId::new(1);
        let other_tx = TransactionId::new(2);

        // Record creator as committed
        status_table
            .record(tx_id, TransactionStatus::Committed)
            .expect("record commit");

        // Register a snapshot at ts=200 (min_visible will be 200)
        registry
            .register_snapshot(SnapshotHandle::new(200, other_tx).expect("snapshot"))
            .expect("register");

        let collector = MvccGarbageCollector::new(
            Arc::new(registry),
            Arc::new(status_table),
        );

        // Version with end_ts < 200 should be reclaimable
        assert!(collector.is_version_reclaimable(tx_id, 199));
        assert!(collector.is_version_reclaimable(tx_id, 100));

        // Version with end_ts >= 200 should NOT be reclaimable
        assert!(!collector.is_version_reclaimable(tx_id, 200));
        assert!(!collector.is_version_reclaimable(tx_id, 250));
    }

    #[test]
    fn test_committed_version_newer_than_min_visible() {
        let mut status_table = TransactionStatusTable::new();
        let registry = ActiveSnapshotRegistry::new();
        let tx_id = TransactionId::new(1);

        // Record creator as committed
        status_table
            .record(tx_id, TransactionStatus::Committed)
            .expect("record commit");

        // No snapshots registered → min_visible = u64::MAX
        let collector = MvccGarbageCollector::new(
            Arc::new(registry),
            Arc::new(status_table),
        );

        // Even very old versions are not reclaimable when min_visible = u64::MAX
        assert!(!collector.is_version_reclaimable(tx_id, 1));
        assert!(!collector.is_version_reclaimable(tx_id, 1_000_000));
    }

    #[test]
    fn test_stats_tracking() {
        let collector = make_collector();

        assert_eq!(collector.get_stats().versions_scanned, 0);
        assert_eq!(collector.get_stats().versions_reclaimed, 0);
        assert_eq!(collector.get_stats().runs, 0);

        collector.record_versions_scanned(100);
        assert_eq!(collector.get_stats().versions_scanned, 100);

        collector.record_versions_reclaimed(25);
        assert_eq!(collector.get_stats().versions_reclaimed, 25);

        collector
            .run_gc()
            .expect("gc run");
        assert_eq!(collector.get_stats().runs, 1);
        assert!(collector.get_stats().last_run_ms > 0);
    }

    #[test]
    fn test_gc_summary_includes_min_visible() {
        let registry = ActiveSnapshotRegistry::new();
        let tx_id = TransactionId::new(1);

        registry
            .register_snapshot(SnapshotHandle::new(500, tx_id).expect("snapshot"))
            .expect("register");

        let collector = MvccGarbageCollector::new(
            Arc::new(registry),
            Arc::new(TransactionStatusTable::new()),
        );

        let summary = collector.run_gc().expect("gc");
        assert_eq!(summary.min_visible_ts, 500);
    }

    #[test]
    fn test_multiple_snapshots_min_visible() {
        let registry = ActiveSnapshotRegistry::new();
        let mut status_table = TransactionStatusTable::new();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let tx3 = TransactionId::new(3);

        // Register three snapshots at different timestamps
        registry
            .register_snapshot(SnapshotHandle::new(500, tx1).expect("snapshot"))
            .expect("register");
        registry
            .register_snapshot(SnapshotHandle::new(200, tx2).expect("snapshot"))
            .expect("register");
        registry
            .register_snapshot(SnapshotHandle::new(800, tx3).expect("snapshot"))
            .expect("register");

        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record");

        let collector = MvccGarbageCollector::new(
            Arc::new(registry),
            Arc::new(status_table),
        );

        // min_visible_ts should be the minimum: 200
        assert_eq!(collector.minimum_visible_timestamp(), 200);

        // Version with end_ts = 199 should be reclaimable
        assert!(collector.is_version_reclaimable(tx1, 199));

        // Version with end_ts = 200 should NOT be reclaimable
        assert!(!collector.is_version_reclaimable(tx1, 200));
    }

    #[test]
    fn test_stats_accumulation() {
        let collector = make_collector();

        collector.record_versions_scanned(50);
        collector.record_versions_reclaimed(10);
        collector.record_versions_scanned(30);
        collector.record_versions_reclaimed(5);

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 80);
        assert_eq!(stats.versions_reclaimed, 15);
    }
}
