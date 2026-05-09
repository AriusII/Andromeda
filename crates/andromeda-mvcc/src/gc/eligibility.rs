//! Garbage Collection Eligibility Checker
//!
//! Determines which row versions are safe to reclaim based on active snapshot
//! visibility. Uses the [`ActiveSnapshotRegistry`] to make GC decisions.

use crate::active_snapshot_registry::ActiveSnapshotRegistry;

/// Garbage Collection Eligibility Checker
///
/// Provides high-level queries for identifying row versions that can be
/// safely reclaimed. All decisions are based solely on the active snapshot
/// registry's minimum visible timestamp and do not depend on wall-clock time
/// or transaction state beyond what is tracked in the registry.
pub struct GcEligibilityChecker {
    registry: ActiveSnapshotRegistry,
}

impl GcEligibilityChecker {
    /// Create a new GC eligibility checker bound to a snapshot registry.
    pub fn new(registry: ActiveSnapshotRegistry) -> Self {
        GcEligibilityChecker { registry }
    }

    /// Check if a single row version is garbageable.
    ///
    /// A version with `end_ts` is garbageable if:
    /// 1. It has been closed (`end_ts ≠ u64::MAX`)
    /// 2. Its `end_ts` is strictly less than the minimum visible timestamp
    ///
    /// # Arguments
    ///
    /// * `end_ts` - The end timestamp of the version. Use `u64::MAX` for
    ///   currently live versions.
    #[inline]
    pub fn is_garbageable(&self, end_ts: u64) -> bool {
        self.registry.is_version_garbageable(end_ts)
    }

    /// Estimate what percentage of versions in a collection are garbageable.
    ///
    /// Returns a value in the range [0.0, 100.0]. Useful for diagnostics
    /// and monitoring garbage collection pressure.
    ///
    /// # Arguments
    ///
    /// * `versions` - A slice of end timestamps for versions of a single row
    pub fn estimate_reclaim_percentage(&self, versions: &[u64]) -> f32 {
        if versions.is_empty() {
            return 0.0;
        }

        let garbageable = versions
            .iter()
            .filter(|&&end_ts| self.is_garbageable(end_ts))
            .count();
        (garbageable as f32) / (versions.len() as f32) * 100.0
    }

    /// Get the current minimum visible timestamp.
    ///
    /// This is the threshold below which all closed versions are garbageable.
    #[inline]
    pub fn minimum_visible_timestamp(&self) -> u64 {
        self.registry.minimum_visible_timestamp()
    }

    /// Get the count of active snapshots currently tracked.
    ///
    /// Higher counts indicate more GC pressure (fewer versions can be reclaimed).
    #[inline]
    pub fn active_snapshot_count(&self) -> usize {
        self.registry.active_snapshot_count()
    }
}

impl Clone for GcEligibilityChecker {
    fn clone(&self) -> Self {
        GcEligibilityChecker {
            registry: self.registry.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_snapshot_registry::SnapshotHandle;
    use andromeda_types::TransactionId;

    #[test]
    fn test_gc_checker_single_snapshot() {
        let registry = ActiveSnapshotRegistry::new();
        let checker = GcEligibilityChecker::new(registry);

        // No snapshots: min is u64::MAX, so closed versions are garbageable.
        assert!(checker.is_garbageable(u64::MAX - 1));

        // Register snapshot at ts 100
        let h = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        checker
            .registry
            .register_snapshot(h)
            .expect("should register snapshot");

        // Versions before min_visible_ts are garbageable
        assert!(checker.is_garbageable(99));
        assert!(checker.is_garbageable(50));
        assert!(checker.is_garbageable(1));

        // Versions at or after min_visible_ts are not garbageable
        assert!(!checker.is_garbageable(100));
        assert!(!checker.is_garbageable(150));

        // Live versions are never garbageable
        assert!(!checker.is_garbageable(u64::MAX));
    }

    #[test]
    fn test_gc_checker_multiple_snapshots() {
        let registry = ActiveSnapshotRegistry::new();
        let checker = GcEligibilityChecker::new(registry);

        // Register snapshots
        let h1 = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        let h2 = SnapshotHandle::new(200, TransactionId::new(2)).unwrap();
        let h3 = SnapshotHandle::new(150, TransactionId::new(3)).unwrap();

        checker.registry.register_snapshot(h1).unwrap();
        checker.registry.register_snapshot(h2).unwrap();
        checker.registry.register_snapshot(h3).unwrap();

        // Min is 100; only versions with end_ts < 100 are garbageable
        assert!(checker.is_garbageable(99));
        assert!(!checker.is_garbageable(100));
        assert!(!checker.is_garbageable(150));
        assert!(!checker.is_garbageable(200));
    }

    #[test]
    fn test_estimate_reclaim_percentage() {
        let registry = ActiveSnapshotRegistry::new();
        let checker = GcEligibilityChecker::new(registry);

        // Register snapshot at ts 100
        let h = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        checker.registry.register_snapshot(h).unwrap();

        // Empty collection
        assert_eq!(checker.estimate_reclaim_percentage(&[]), 0.0);

        // All garbageable
        assert_eq!(checker.estimate_reclaim_percentage(&[50, 75, 99]), 100.0);

        // None garbageable
        assert_eq!(
            checker.estimate_reclaim_percentage(&[100, 150, u64::MAX]),
            0.0
        );

        // Mixed
        let versions = vec![50, 100, 75, 150, 99];
        let percentage = checker.estimate_reclaim_percentage(&versions);
        // 3 out of 5 are garbageable (50, 75, 99)
        assert!((percentage - 60.0).abs() < 0.01);
    }

    #[test]
    fn test_minimum_visible_timestamp_query() {
        let registry = ActiveSnapshotRegistry::new();
        let checker = GcEligibilityChecker::new(registry);

        assert_eq!(checker.minimum_visible_timestamp(), u64::MAX);

        let h = SnapshotHandle::new(500, TransactionId::new(1)).unwrap();
        checker.registry.register_snapshot(h).unwrap();

        assert_eq!(checker.minimum_visible_timestamp(), 500);
    }

    #[test]
    fn test_active_snapshot_count_query() {
        let registry = ActiveSnapshotRegistry::new();
        let checker = GcEligibilityChecker::new(registry);

        assert_eq!(checker.active_snapshot_count(), 0);

        let h1 = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        let h2 = SnapshotHandle::new(200, TransactionId::new(2)).unwrap();

        checker.registry.register_snapshot(h1).unwrap();
        assert_eq!(checker.active_snapshot_count(), 1);

        checker.registry.register_snapshot(h2).unwrap();
        assert_eq!(checker.active_snapshot_count(), 2);

        checker.registry.release_snapshot(h1).unwrap();
        assert_eq!(checker.active_snapshot_count(), 1);
    }

    #[test]
    fn test_gc_checker_clone_shares_registry() {
        let registry = ActiveSnapshotRegistry::new();
        let checker1 = GcEligibilityChecker::new(registry);
        let checker2 = checker1.clone();

        let h = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        checker1.registry.register_snapshot(h).unwrap();

        // Both checkers see the same state
        assert_eq!(checker2.minimum_visible_timestamp(), 100);
    }
}
