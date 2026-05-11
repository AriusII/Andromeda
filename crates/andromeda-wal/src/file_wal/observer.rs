//! Production WAL durability observer backed by [`FileWal`]'s flush-through signal.
//!
//! [`FileWalDurabilityObserver`] wraps an `Arc<AtomicU64>` that is atomically
//! advanced by [`FileWal::flush_through`] **after** `sync_data()` returns —
//! i.e., once WAL records are physically durable on disk.  Buffer-pool callers
//! obtain an observer via [`FileWal::observer`] and pass it to
//! `BufferPool::flush_all_dirty_with_report` or `flush_dirty_frames`.
//!
//! # Memory ordering protocol
//!
//! ```text
//! Writer (FileWal::flush_through):
//!   write_all(record bytes)
//!   → file.sync_data()                     [OS-level flush guarantee]
//!   → shared_lsn.store(lsn, Release)       [signal advance — Release]
//!   → write_file_wal_header()
//!   → file.sync_all()                      [header durability]
//!
//! Reader (FileWalDurabilityObserver::current_durable_lsn):
//!   shared_lsn.load(Acquire)               [sees all writes ≤ the Release store]
//! ```
//!
//! The `Release`/`Acquire` pair ensures that any observer reading the advanced
//! LSN is guaranteed to see all WAL bytes synced by the corresponding
//! `flush_through` call.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Lsn;

/// Production WAL durability signal wired to [`FileWal::flush_through`].
///
/// Obtain instances via [`FileWal::observer`].  The observer is cheaply
/// cloneable through `Arc` semantics: multiple buffer-pool callers can each
/// hold an independent handle while a single `FileWal` writer advances the
/// underlying signal.
///
/// This type implements [`andromeda_buffer_pool::WalDurabilityObserver`] via a
/// blanket impl in `andromeda-buffer-pool` (which owns that trait), so it may
/// be passed directly to `BufferPool::flush_all_dirty_with_report`.
#[derive(Debug, Clone)]
pub struct FileWalDurabilityObserver {
    shared_durable_lsn: Arc<AtomicU64>,
}

impl FileWalDurabilityObserver {
    /// Construct an observer from an existing shared atomic writer arc.
    ///
    /// Called exclusively by [`FileWal::observer`]; not part of the public API.
    pub(super) fn from_shared(shared: Arc<AtomicU64>) -> Self {
        Self {
            shared_durable_lsn: shared,
        }
    }

    /// Non-blocking read of the latest LSN that is durably flushed to the WAL.
    ///
    /// Uses `Acquire` memory ordering: callers observing a returned value `v`
    /// are guaranteed to see all WAL byte writes that preceded the
    /// `flush_through` call that advanced the signal to `v`.
    #[must_use]
    pub fn current_durable_lsn(&self) -> Lsn {
        Lsn::new(self.shared_durable_lsn.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn observer_reports_initial_lsn_correctly() {
        let shared = Arc::new(AtomicU64::new(42));
        let obs = FileWalDurabilityObserver::from_shared(Arc::clone(&shared));
        assert_eq!(obs.current_durable_lsn(), Lsn::new(42));
    }

    #[test]
    fn observer_sees_advance_through_shared_arc() {
        let shared = Arc::new(AtomicU64::new(0));
        let obs = FileWalDurabilityObserver::from_shared(Arc::clone(&shared));

        assert_eq!(obs.current_durable_lsn(), Lsn::new(0));
        shared.store(100, Ordering::Release);
        assert_eq!(obs.current_durable_lsn(), Lsn::new(100));
    }

    #[test]
    fn cloned_observer_shares_the_same_signal() {
        let shared = Arc::new(AtomicU64::new(0));
        let obs_a = FileWalDurabilityObserver::from_shared(Arc::clone(&shared));
        let obs_b = obs_a.clone();

        shared.store(77, Ordering::Release);
        assert_eq!(obs_a.current_durable_lsn(), Lsn::new(77));
        assert_eq!(obs_b.current_durable_lsn(), Lsn::new(77));
    }
}
