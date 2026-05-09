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

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_types::TransactionId;
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
    pub fn allocate(&self) -> AndromedaResult<TransactionId> {
        let previous = self
            .last_issued
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .map_err(|_| {
                AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "transaction id allocator exhausted durable id space",
                )
            })?;

        let next = previous.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id allocator exhausted durable id space",
            )
        })?;

        Ok(TransactionId::new(next))
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
        assert_eq!(allocate(&allocator), 1);
        assert_eq!(allocate(&allocator), 2);
        assert_eq!(allocate(&allocator), 3);
        assert_eq!(allocator.peek_last_issued(), 3);
    }

    #[test]
    fn never_returns_zero() {
        let allocator = TransactionIdAllocator::new();
        for _ in 0..16 {
            assert_ne!(allocate(&allocator), 0);
        }
    }

    #[test]
    fn seed_advances_floor_and_keeps_monotonicity() {
        let allocator = TransactionIdAllocator::new();
        allocator
            .seed(100)
            .expect("seeding allocator floor should succeed");
        assert_eq!(allocate(&allocator), 101);
        // Re-seeding to a higher floor still works.
        allocator
            .seed(500)
            .expect("raising allocator floor should succeed");
        assert_eq!(allocate(&allocator), 501);
    }

    #[test]
    fn seed_rejects_backwards_floor() {
        let allocator = TransactionIdAllocator::with_floor(50);
        let err = allocator
            .seed(10)
            .expect_err("lowering allocator floor must fail");
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        // The floor must not have moved.
        assert_eq!(allocate(&allocator), 51);
    }

    #[test]
    fn allocate_returns_typed_error_at_id_space_exhaustion() {
        let allocator = TransactionIdAllocator::with_floor(u64::MAX - 1);

        assert_eq!(allocate(&allocator), u64::MAX);
        let err = allocator
            .allocate()
            .expect_err("exhausted transaction id allocator must fail");

        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(allocator.peek_last_issued(), u64::MAX);
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
                    local.push(allocate(&allocator));
                }
                local
            }));
        }

        let mut all = Vec::new();
        for handle in handles {
            all.extend(handle.join().expect("allocator worker should not panic"));
        }

        let unique: HashSet<u64> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "transaction ids collided");
        assert!(unique.iter().all(|id| *id != 0));
        assert_eq!(
            *unique.iter().max().expect("allocator should produce ids"),
            8 * 256
        );
    }

    fn allocate(allocator: &TransactionIdAllocator) -> u64 {
        allocator
            .allocate()
            .expect("transaction id allocation should succeed")
            .get()
    }
}
