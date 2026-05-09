#![forbid(unsafe_code)]
#![doc = r#"
Boundary crate for Andromeda buffer-pool coordination.

This crate owns buffer-pool boundary contracts that are independent of page
layout and disk I/O implementation. Storage keeps the resident frame machinery
during the migration and imports these contracts through its compatibility
facade.

C5 invariants:

- Dirty page flush must not outrun durable WAL coverage through the page LSN.
- In-memory residency is advisory and must not be treated as durable truth.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use andromeda_wal::Lsn;

mod clock;
mod config;
mod dirty;
mod error;
mod flush;
mod flush_result;
mod frame;
mod guard;
mod manager;
mod page_dirty;
mod pool_config;
mod pool_error;
mod resident_frame;

pub use clock::{ClockEvictionCandidate, ClockEvictionPolicy, ClockFrame};
pub use config::BufferPoolFrameConfig;
pub use dirty::{
    DirtyEntry as DirtyEntryCore, DirtyFlushCandidate as DirtyFlushCandidateCore,
    DirtyTracker as DirtyTrackerCore,
};
pub use error::BufferPoolCoreError;
pub use flush::{FlushBlockedFrameCore, FlushReadiness, classify_flush_candidate};
pub use flush_result::{FlushAllDirtyResult, FlushBlockedFrame, FlushError, FlushStorageOperation};
pub use frame::{BufferFrameCore, BufferFrameId, BufferFrameState};
pub use guard::{PageGuard, PageGuardMut};
pub use manager::{BufferPool, BufferPoolManager};
pub use page_dirty::{DirtyEntry, DirtyFlushCandidate, DirtyTracker};
pub use pool_config::BufferPoolConfig;
pub use pool_error::BufferPoolError;
pub use resident_frame::BufferFrame;

/// Observer contract for WAL durability tracking.
///
/// Implementations report which LSNs have been made durable in the WAL. Buffer
/// pool implementations use this boundary to gate dirty page flushes.
pub trait WalDurabilityObserver: Send + Sync {
    /// Check whether an LSN is durable in the WAL.
    fn is_durable(&self, lsn: Lsn) -> bool;

    /// Return the maximum LSN known to be durable.
    fn max_durable_lsn(&self) -> Lsn;
}

/// Deterministic observer for tests and compatibility scaffolding.
#[derive(Debug, Clone)]
pub struct TestWalDurabilityObserver {
    durable_lsn: Arc<AtomicU64>,
}

impl TestWalDurabilityObserver {
    /// Create a new observer with only LSN zero durable.
    pub fn new() -> Self {
        Self {
            durable_lsn: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new observer with a specific durable LSN.
    pub fn with_durable_lsn(durable_lsn: u64) -> Self {
        Self {
            durable_lsn: Arc::new(AtomicU64::new(durable_lsn)),
        }
    }

    /// Update the durable LSN.
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
        Lsn::new(self.durable_lsn.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observer_initially_reports_only_zero_durable() {
        let observer = TestWalDurabilityObserver::new();

        assert_eq!(observer.max_durable_lsn(), Lsn::new(0));
        assert!(observer.is_durable(Lsn::new(0)));
        assert!(!observer.is_durable(Lsn::new(1)));
    }

    #[test]
    fn observer_respects_durable_lsn_updates() {
        let observer = TestWalDurabilityObserver::with_durable_lsn(50);

        assert!(observer.is_durable(Lsn::new(49)));
        assert!(observer.is_durable(Lsn::new(50)));
        assert!(!observer.is_durable(Lsn::new(51)));

        observer.set_durable_lsn(100);
        assert_eq!(observer.max_durable_lsn(), Lsn::new(100));
        observer.advance_durable_lsn();
        assert_eq!(observer.max_durable_lsn(), Lsn::new(101));
    }
}
