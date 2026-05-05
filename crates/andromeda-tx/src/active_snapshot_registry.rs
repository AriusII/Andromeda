//! Active Snapshot Registry for MVCC Garbage Collection
//!
//! # Overview
//!
//! The [`ActiveSnapshotRegistry`] tracks which MVCC snapshots are currently
//! in use by active transactions. It enables safe garbage collection of old
//! row versions by maintaining the **minimum visible timestamp** (the oldest
//! active snapshot's begin_ts). Row versions with `end_ts < min_visible_ts`
//! and `end_ts ≠ INF` are garbageable because no active snapshot can see them.
//!
//! # Thread Safety
//!
//! The registry is fully thread-safe:
//! - Snapshot register/release operations use RwLock for mutation safety
//! - Minimum visible timestamp is cached with Mutex for lock-free reads
//! - Arc-wrapped for shared ownership across threads
//!
//! # Invariants
//!
//! 1. A snapshot can be registered only once (duplicate registration fails)
//! 2. A snapshot can only be released if it was registered (unregistered release fails)
//! 3. `minimum_visible_timestamp()` is always the minimum of all active `begin_ts` values
//! 4. A version is garbageable iff `end_ts < min_visible_ts` AND `end_ts ≠ INF`
//! 5. No unsafe code; all operations are fallible with explicit error handling

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::RwLock as StdRwLock;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

/// Immutable snapshot reference for GC queries.
///
/// Contains the minimum information needed to track snapshot lifetime and
/// compute garbage collection eligibility. The `begin_ts` represents the
/// logical commit-order timestamp when this snapshot was created.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SnapshotHandle {
    /// The logical timestamp when the snapshot was created.
    pub begin_ts: u64,
    /// Transaction id owning this snapshot (for debugging/tracing).
    pub tx_id: TransactionId,
}

impl SnapshotHandle {
    /// Create a new snapshot handle.
    ///
    /// # Errors
    ///
    /// Returns an error if `begin_ts` is 0 (sentinel value not allowed).
    pub fn new(begin_ts: u64, tx_id: TransactionId) -> AndromedaResult<Self> {
        if begin_ts == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot begin timestamp must not be zero",
            ));
        }

        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot transaction id must not be zero",
            ));
        }

        Ok(SnapshotHandle { begin_ts, tx_id })
    }
}

/// Error types for Active Snapshot Registry operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcError {
    /// A snapshot with this (begin_ts, tx_id) is already registered.
    SnapshotAlreadyRegistered,
    /// A snapshot with this (begin_ts, tx_id) is not registered.
    SnapshotNotFound,
}

impl std::fmt::Display for GcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GcError::SnapshotAlreadyRegistered => {
                write!(f, "snapshot already registered in active registry")
            }
            GcError::SnapshotNotFound => {
                write!(f, "snapshot not found in active registry")
            }
        }
    }
}

impl std::error::Error for GcError {}

/// Active snapshots registry for MVCC garbage collection eligibility.
///
/// Tracks which MVCC snapshots are currently in use and computes the
/// **minimum visible timestamp** — the oldest active snapshot's begin_ts.
/// This enables safe identification of row versions that can be reclaimed.
pub struct ActiveSnapshotRegistry {
    /// Registry of active (begin_ts, tx_id) pairs.
    /// Guarded by RwLock to allow concurrent snapshot tracking with efficient reads.
    active: Arc<StdRwLock<HashSet<SnapshotHandle>>>,

    /// Cached minimum visible timestamp, updated on register/release.
    /// Uses Mutex (not RwLock) because we only cache a single u64.
    cached_min_visible_ts: Arc<StdMutex<u64>>,
}

impl Clone for ActiveSnapshotRegistry {
    fn clone(&self) -> Self {
        ActiveSnapshotRegistry {
            active: Arc::clone(&self.active),
            cached_min_visible_ts: Arc::clone(&self.cached_min_visible_ts),
        }
    }
}

/// Sentinel value representing "no minimum timestamp" (all snapshots released).
const MIN_TS_INF: u64 = u64::MAX;

impl ActiveSnapshotRegistry {
    /// Create a new, empty snapshot registry.
    pub fn new() -> Self {
        ActiveSnapshotRegistry {
            active: Arc::new(StdRwLock::new(HashSet::new())),
            cached_min_visible_ts: Arc::new(StdMutex::new(MIN_TS_INF)),
        }
    }

    /// Register a snapshot at transaction start.
    ///
    /// Adds the snapshot to the active set and recalculates the minimum visible
    /// timestamp. Must be called exactly once per snapshot lifetime.
    ///
    /// # Errors
    ///
    /// Returns `GcError::SnapshotAlreadyRegistered` if a snapshot with the same
    /// `begin_ts` and `tx_id` is already registered.
    pub fn register_snapshot(&self, handle: SnapshotHandle) -> Result<(), GcError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| GcError::SnapshotAlreadyRegistered)?;

        if active.contains(&handle) {
            return Err(GcError::SnapshotAlreadyRegistered);
        }

        active.insert(handle);

        // Recalculate minimum visible timestamp
        let new_min = active
            .iter()
            .map(|h| h.begin_ts)
            .min()
            .unwrap_or(MIN_TS_INF);

        let mut cached = self
            .cached_min_visible_ts
            .lock()
            .map_err(|_| GcError::SnapshotAlreadyRegistered)?;
        *cached = new_min;

        Ok(())
    }

    /// Release a snapshot at transaction end (commit or rollback).
    ///
    /// Removes the snapshot from the active set and recalculates the minimum
    /// visible timestamp. Must be called exactly once per registered snapshot.
    ///
    /// # Errors
    ///
    /// Returns `GcError::SnapshotNotFound` if the snapshot is not currently
    /// registered.
    pub fn release_snapshot(&self, handle: SnapshotHandle) -> Result<(), GcError> {
        let mut active = self.active.write().map_err(|_| GcError::SnapshotNotFound)?;

        if !active.remove(&handle) {
            return Err(GcError::SnapshotNotFound);
        }

        // Recalculate minimum visible timestamp
        let new_min = active
            .iter()
            .map(|h| h.begin_ts)
            .min()
            .unwrap_or(MIN_TS_INF);

        let mut cached = self
            .cached_min_visible_ts
            .lock()
            .map_err(|_| GcError::SnapshotNotFound)?;
        *cached = new_min;

        Ok(())
    }

    /// Query the minimum visible timestamp across all active snapshots.
    ///
    /// Returns `u64::MAX` if no snapshots are active.
    /// Row versions with `end_ts < minimum_visible_timestamp()` AND
    /// `end_ts ≠ u64::MAX` are candidates for garbage collection.
    #[inline]
    pub fn minimum_visible_timestamp(&self) -> u64 {
        self.cached_min_visible_ts
            .lock()
            .map(|guard| *guard)
            .unwrap_or(MIN_TS_INF)
    }

    /// Check if a row version is garbageable.
    ///
    /// A version is garbageable if:
    /// 1. It has been closed (`end_ts ≠ u64::MAX`)
    /// 2. Its `end_ts` is strictly less than the minimum visible timestamp
    ///
    /// This means no active snapshot can see the version.
    pub fn is_version_garbageable(&self, end_ts: u64) -> bool {
        end_ts != MIN_TS_INF && end_ts < self.minimum_visible_timestamp()
    }

    /// Count the number of currently active snapshots.
    ///
    /// Useful for diagnostics and monitoring GC pressure.
    pub fn active_snapshot_count(&self) -> usize {
        self.active.read().map(|guard| guard.len()).unwrap_or(0)
    }

    /// List all active begin timestamps.
    ///
    /// Primarily useful for diagnostics and debugging. Returns a snapshot
    /// of active timestamps at the moment of the call; may be stale
    /// immediately after return due to concurrent operations.
    pub fn active_begin_timestamps(&self) -> Vec<u64> {
        self.active
            .read()
            .map(|guard| guard.iter().map(|h| h.begin_ts).collect())
            .unwrap_or_default()
    }

    /// List all active snapshot handles.
    ///
    /// Primarily useful for diagnostics. Returns a snapshot of active handles
    /// at the moment of the call.
    pub fn active_handles(&self) -> Vec<SnapshotHandle> {
        self.active
            .read()
            .map(|guard| guard.iter().copied().collect())
            .unwrap_or_default()
    }
}

impl Default for ActiveSnapshotRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_handle_creation_rejects_zero_timestamp() {
        assert!(SnapshotHandle::new(0, TransactionId::new(1)).is_err());
    }

    #[test]
    fn test_snapshot_handle_creation_rejects_zero_tx_id() {
        assert!(SnapshotHandle::new(100, TransactionId::new(0)).is_err());
    }

    #[test]
    fn test_snapshot_handle_creation_succeeds() {
        let handle = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        assert_eq!(handle.begin_ts, 100);
        assert_eq!(handle.tx_id, TransactionId::new(1));
    }

    #[test]
    fn test_registry_register_and_release() {
        let registry = ActiveSnapshotRegistry::new();

        let handle = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();

        // Initial state: no snapshots
        assert_eq!(registry.active_snapshot_count(), 0);
        assert_eq!(registry.minimum_visible_timestamp(), MIN_TS_INF);

        // Register snapshot
        assert!(registry.register_snapshot(handle).is_ok());
        assert_eq!(registry.active_snapshot_count(), 1);
        assert_eq!(registry.minimum_visible_timestamp(), 100);

        // Release snapshot
        assert!(registry.release_snapshot(handle).is_ok());
        assert_eq!(registry.active_snapshot_count(), 0);
        assert_eq!(registry.minimum_visible_timestamp(), MIN_TS_INF);
    }

    #[test]
    fn test_registry_duplicate_register_error() {
        let registry = ActiveSnapshotRegistry::new();

        let handle = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();

        assert!(registry.register_snapshot(handle).is_ok());
        assert_eq!(
            registry.register_snapshot(handle),
            Err(GcError::SnapshotAlreadyRegistered)
        );
    }

    #[test]
    fn test_registry_release_not_found_error() {
        let registry = ActiveSnapshotRegistry::new();

        let handle = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        assert_eq!(
            registry.release_snapshot(handle),
            Err(GcError::SnapshotNotFound)
        );
    }

    #[test]
    fn test_registry_minimum_visible_timestamp_multiple_snapshots() {
        let registry = ActiveSnapshotRegistry::new();

        // No snapshots: min is INF
        assert_eq!(registry.minimum_visible_timestamp(), MIN_TS_INF);

        // Add snapshot at ts 100
        let h1 = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        registry.register_snapshot(h1).unwrap();
        assert_eq!(registry.minimum_visible_timestamp(), 100);

        // Add snapshot at ts 50 (becomes min)
        let h2 = SnapshotHandle::new(50, TransactionId::new(2)).unwrap();
        registry.register_snapshot(h2).unwrap();
        assert_eq!(registry.minimum_visible_timestamp(), 50);

        // Add snapshot at ts 75 (doesn't affect min)
        let h3 = SnapshotHandle::new(75, TransactionId::new(3)).unwrap();
        registry.register_snapshot(h3).unwrap();
        assert_eq!(registry.minimum_visible_timestamp(), 50);

        // Release h2 (50 is removed, min becomes 75)
        registry.release_snapshot(h2).unwrap();
        assert_eq!(registry.minimum_visible_timestamp(), 75);

        // Release h3 (min becomes 100)
        registry.release_snapshot(h3).unwrap();
        assert_eq!(registry.minimum_visible_timestamp(), 100);

        // Release h1 (no more snapshots, min becomes INF)
        registry.release_snapshot(h1).unwrap();
        assert_eq!(registry.minimum_visible_timestamp(), MIN_TS_INF);
    }

    #[test]
    fn test_garbage_collection_version_garbageable() {
        let registry = ActiveSnapshotRegistry::new();

        let h = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        registry.register_snapshot(h).unwrap();

        // Version that ended before snapshot began: garbageable
        assert!(registry.is_version_garbageable(99));

        // Version that's still alive (INF): NOT garbageable
        assert!(!registry.is_version_garbageable(MIN_TS_INF));

        // Version that ended after snapshot: NOT garbageable
        assert!(!registry.is_version_garbageable(150));

        // Version that ended at exact min_visible_ts: NOT garbageable
        assert!(!registry.is_version_garbageable(100));
    }

    #[test]
    fn test_garbage_collection_with_multiple_snapshots() {
        let registry = ActiveSnapshotRegistry::new();

        let h1 = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        let h2 = SnapshotHandle::new(200, TransactionId::new(2)).unwrap();
        let h3 = SnapshotHandle::new(150, TransactionId::new(3)).unwrap();

        registry.register_snapshot(h1).unwrap();
        registry.register_snapshot(h2).unwrap();
        registry.register_snapshot(h3).unwrap();

        // Min is 100; only versions with end_ts < 100 are garbageable
        assert!(registry.is_version_garbageable(99));
        assert!(registry.is_version_garbageable(50));
        assert!(!registry.is_version_garbageable(100));
        assert!(!registry.is_version_garbageable(150));
        assert!(!registry.is_version_garbageable(200));
    }

    #[test]
    fn test_active_begin_timestamps() {
        let registry = ActiveSnapshotRegistry::new();

        let h1 = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        let h2 = SnapshotHandle::new(50, TransactionId::new(2)).unwrap();
        let h3 = SnapshotHandle::new(75, TransactionId::new(3)).unwrap();

        registry.register_snapshot(h1).unwrap();
        registry.register_snapshot(h2).unwrap();
        registry.register_snapshot(h3).unwrap();

        let mut timestamps = registry.active_begin_timestamps();
        timestamps.sort_unstable();

        assert_eq!(timestamps, vec![50, 75, 100]);
    }

    #[test]
    fn test_active_handles() {
        let registry = ActiveSnapshotRegistry::new();

        let h1 = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        let h2 = SnapshotHandle::new(50, TransactionId::new(2)).unwrap();

        registry.register_snapshot(h1).unwrap();
        registry.register_snapshot(h2).unwrap();

        let handles = registry.active_handles();
        assert_eq!(handles.len(), 2);
        assert!(handles.contains(&h1));
        assert!(handles.contains(&h2));
    }

    #[test]
    fn test_clone_registry_shares_state() {
        let registry1 = ActiveSnapshotRegistry::new();
        let registry2 = registry1.clone();

        let h = SnapshotHandle::new(100, TransactionId::new(1)).unwrap();
        registry1.register_snapshot(h).unwrap();

        // Both registries share the same underlying state
        assert_eq!(registry2.active_snapshot_count(), 1);
        assert_eq!(registry2.minimum_visible_timestamp(), 100);
    }

    #[test]
    fn test_many_snapshots_performance() {
        let registry = ActiveSnapshotRegistry::new();

        // Register many snapshots
        for i in 1..=1000 {
            let h = SnapshotHandle::new(i as u64, TransactionId::new(i)).unwrap();
            registry.register_snapshot(h).unwrap();
        }

        assert_eq!(registry.active_snapshot_count(), 1000);
        assert_eq!(registry.minimum_visible_timestamp(), 1);

        // Verify GC eligibility
        assert!(registry.is_version_garbageable(0)); // Would be, but 0 is invalid timestamp
        assert!(!registry.is_version_garbageable(1));
        assert!(!registry.is_version_garbageable(500));
        assert!(!registry.is_version_garbageable(1000));
    }
}
