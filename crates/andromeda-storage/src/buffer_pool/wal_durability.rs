//! WAL durability observer for LSN-safe dirty flush gate.
//!
//! This module defines the interface between the buffer pool and WAL subsystem.
//! Before flushing a dirty page, the buffer pool must verify that the page's
//! latest dirty LSN is durable in the WAL.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Lsn;

/// Observer contract for WAL durability tracking.
///
/// Implementations report which LSNs have been made durable in the WAL.
/// The buffer pool uses this to gate flush operations: a page can only be
/// flushed once its latest dirty LSN is confirmed durable.
pub trait WalDurabilityObserver: Send + Sync {
    /// Check whether an LSN is durable in the WAL.
    fn is_durable(&self, lsn: Lsn) -> bool;

    /// Return the maximum LSN known to be durable.
    fn max_durable_lsn(&self) -> Lsn;
}

/// Test implementation of WAL durability observer.
///
/// Provides deterministic control over which LSN is durable for testing
/// the flush gate behavior.
#[derive(Debug, Clone)]
pub struct TestWalDurabilityObserver {
    durable_lsn: Arc<AtomicU64>,
}

impl TestWalDurabilityObserver {
    /// Create a new test observer with zero LSN durable (no durability).
    pub fn new() -> Self {
        Self {
            durable_lsn: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new test observer with a specific durable LSN.
    pub fn with_durable_lsn(durable_lsn: u64) -> Self {
        Self {
            durable_lsn: Arc::new(AtomicU64::new(durable_lsn)),
        }
    }

    /// Update the durable LSN (for test simulation).
    pub fn set_durable_lsn(&self, lsn: u64) {
        self.durable_lsn.store(lsn, Ordering::Release);
    }

    /// Advance the durable LSN by one.
    pub fn advance_durable_lsn(&self) {
        let current = self.durable_lsn.load(Ordering::Acquire);
        self.durable_lsn.store(current + 1, Ordering::Release);
    }
}

impl Default for TestWalDurabilityObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl WalDurabilityObserver for TestWalDurabilityObserver {
    fn is_durable(&self, lsn: Lsn) -> bool {
        let durable_value = self.durable_lsn.load(Ordering::Acquire);
        lsn.get() <= durable_value
    }

    fn max_durable_lsn(&self) -> Lsn {
        let value = self.durable_lsn.load(Ordering::Acquire);
        Lsn::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_initially_reports_nothing_durable() {
        let observer = TestWalDurabilityObserver::new();
        assert_eq!(observer.max_durable_lsn(), Lsn::new(0));
        assert!(!observer.is_durable(Lsn::new(1)));
        assert!(observer.is_durable(Lsn::new(0)));
    }

    #[test]
    fn test_observer_respects_set_durable_lsn() {
        let observer = TestWalDurabilityObserver::with_durable_lsn(50);
        assert_eq!(observer.max_durable_lsn(), Lsn::new(50));
        assert!(observer.is_durable(Lsn::new(50)));
        assert!(observer.is_durable(Lsn::new(49)));
        assert!(!observer.is_durable(Lsn::new(51)));
    }

    #[test]
    fn test_observer_updates_durable_lsn() {
        let observer = TestWalDurabilityObserver::new();
        observer.set_durable_lsn(100);
        assert_eq!(observer.max_durable_lsn(), Lsn::new(100));
        assert!(observer.is_durable(Lsn::new(100)));
    }

    #[test]
    fn test_observer_advances_durable_lsn() {
        let observer = TestWalDurabilityObserver::with_durable_lsn(10);
        observer.advance_durable_lsn();
        assert_eq!(observer.max_durable_lsn(), Lsn::new(11));
        assert!(observer.is_durable(Lsn::new(11)));
    }

    #[test]
    fn test_observer_comparison_semantics() {
        let observer = TestWalDurabilityObserver::with_durable_lsn(60);
        assert!(observer.is_durable(Lsn::new(30)));
        assert!(observer.is_durable(Lsn::new(60)));
        assert!(!observer.is_durable(Lsn::new(61)));
        assert!(!observer.is_durable(Lsn::new(70)));
    }
}
