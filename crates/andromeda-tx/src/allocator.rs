//! Recovery-safe transaction id allocator.
//!
//! Production code must never derive a [`TransactionId`] from another
//! identifier domain (such as `InvocationId`). Doing so couples unrelated
//! identifier spaces and makes recovery impossible to reason about because
//! transaction ids cease to be monotonic across restarts.
//!
//! The allocator owns a single monotonic counter. After recovery the WAL
//! recovery driver is expected to call [`TransactionIdAllocator::seed`] with
//! the highest transaction id that durably appeared in the log, guaranteeing
//! that any newly minted id is strictly greater than every id that was ever
//! made durable.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic, thread-safe transaction id source.
///
/// The allocator never returns `TransactionId(0)` because zero is reserved
/// across the system as "no transaction".
#[derive(Debug)]
pub struct TransactionIdAllocator {
    /// Highest id that has been handed out so far (0 means none yet).
    last_issued: AtomicU64,
}

impl TransactionIdAllocator {
    /// Create a fresh allocator that will issue ids starting at `1`.
    pub const fn new() -> Self {
        Self {
            last_issued: AtomicU64::new(0),
        }
    }

    /// Create an allocator pre-seeded so the next id is `floor + 1`.
    pub const fn with_floor(floor: u64) -> Self {
        Self {
            last_issued: AtomicU64::new(floor),
        }
    }

    /// Allocate a new transaction id strictly greater than every previously
    /// issued id and every value previously passed to [`Self::seed`].
    pub fn allocate(&self) -> TransactionId {
        // `fetch_add` on a monotonic counter is sufficient: even under
        // contention the produced ids are unique and strictly increasing.
        let next = self.last_issued.fetch_add(1, Ordering::SeqCst) + 1;
        debug_assert!(next != 0, "transaction id counter overflowed");
        TransactionId::new(next)
    }

    /// Raise the floor for future allocations during recovery.
    ///
    /// The provided value represents the highest transaction id observed in
    /// durable evidence (e.g. the WAL). After this call any future
    /// [`Self::allocate`] returns a value strictly greater than `floor`.
    ///
    /// Lowering the floor is rejected; it would risk reissuing an id that
    /// already appears in durable storage.
    pub fn seed(&self, floor: u64) -> AndromedaResult<()> {
        let mut current = self.last_issued.load(Ordering::SeqCst);
        loop {
            if floor < current {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "transaction id allocator floor cannot move backwards",
                ));
            }
            match self.last_issued.compare_exchange(
                current,
                floor,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Ok(()),
                Err(observed) => current = observed,
            }
        }
    }

    /// Inspect the highest id issued so far without allocating.
    pub fn peek_last_issued(&self) -> u64 {
        self.last_issued.load(Ordering::SeqCst)
    }
}

impl Default for TransactionIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn allocate_starts_at_one_and_is_monotonic() {
        let allocator = TransactionIdAllocator::new();
        assert_eq!(allocator.allocate().get(), 1);
        assert_eq!(allocator.allocate().get(), 2);
        assert_eq!(allocator.allocate().get(), 3);
        assert_eq!(allocator.peek_last_issued(), 3);
    }

    #[test]
    fn never_returns_zero() {
        let allocator = TransactionIdAllocator::new();
        for _ in 0..16 {
            assert_ne!(allocator.allocate().get(), 0);
        }
    }

    #[test]
    fn seed_advances_floor_and_keeps_monotonicity() {
        let allocator = TransactionIdAllocator::new();
        allocator.seed(100).unwrap();
        assert_eq!(allocator.allocate().get(), 101);
        // Re-seeding to a higher floor still works.
        allocator.seed(500).unwrap();
        assert_eq!(allocator.allocate().get(), 501);
    }

    #[test]
    fn seed_rejects_backwards_floor() {
        let allocator = TransactionIdAllocator::with_floor(50);
        let err = allocator.seed(10).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        // The floor must not have moved.
        assert_eq!(allocator.allocate().get(), 51);
    }

    #[test]
    fn concurrent_allocation_produces_unique_monotonic_ids() {
        let allocator = Arc::new(TransactionIdAllocator::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let allocator = Arc::clone(&allocator);
            handles.push(thread::spawn(move || {
                let mut local = Vec::with_capacity(256);
                for _ in 0..256 {
                    local.push(allocator.allocate().get());
                }
                local
            }));
        }

        let mut all = Vec::new();
        for handle in handles {
            all.extend(handle.join().unwrap());
        }

        let unique: HashSet<u64> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "transaction ids collided");
        assert!(unique.iter().all(|id| *id != 0));
        assert_eq!(*unique.iter().max().unwrap(), 8 * 256);
    }
}
