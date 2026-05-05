//! Lock manager API.
//!
//! This module defines the storage-agnostic lock vocabulary and a minimal,
//! panic-free manager. C3-LM-002 defines the V0 lock-mode compatibility matrix;
//! C3-LM-003 adds nonblocking acquire decisions. C3-LM-004 adds conservative
//! same-transaction re-entry and nonblocking upgrade rules. C3-LM-005 adds
//! deterministic FIFO waiter promotion on release. Deadlock detection remains
//! deferred.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

/// Lock mode vocabulary for transaction concurrency control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockMode {
    Shared,
    Exclusive,
    IntentShared,
    IntentExclusive,
    SchemaShared,
    SchemaExclusive,
}

impl LockMode {
    /// Return whether an already-held lock mode can coexist with a newly
    /// requested lock mode on the same resource under the V0 matrix.
    ///
    /// The check is deterministic and transaction-agnostic: it deliberately
    /// treats `existing` and `requested` as locks held/requested by different
    /// transactions. Same-transaction conversion, re-entrant acquisition,
    /// fairness, waiter promotion, and deadlock handling are handled or deferred
    /// by the acquire/release layer rather than by this matrix.
    ///
    /// V0 uses conservative schema/data semantics:
    /// - [`LockMode::Exclusive`] conflicts with every other holder, including
    ///   another `Exclusive`, until a later same-transaction acquire rule can
    ///   safely override that at a higher layer.
    /// - [`LockMode::SchemaExclusive`] conflicts with every mode.
    /// - [`LockMode::Shared`] is compatible with `Shared`, `IntentShared`, and
    ///   `SchemaShared`; it conflicts with data/schema exclusivity intent.
    /// - Intent modes coordinate multi-granularity locking: `IntentShared` can
    ///   coexist with shared readers and both intent modes, while
    ///   `IntentExclusive` coexists only with intent/schema-stability holders
    ///   and does not coexist with data `Shared` or `Exclusive` holders on the
    ///   same resource.
    /// - [`LockMode::SchemaShared`] is a schema-stability mode that is broadly
    ///   compatible with non-exclusive data and intent work, but not with
    ///   `Exclusive` or `SchemaExclusive`.
    pub const fn is_compatible_with(self, requested: LockMode) -> bool {
        use LockMode::{IntentExclusive, IntentShared, SchemaShared, Shared};

        matches!(
            (self, requested),
            (Shared, Shared)
                | (Shared, IntentShared)
                | (Shared, SchemaShared)
                | (IntentShared, Shared)
                | (IntentShared, IntentShared)
                | (IntentShared, IntentExclusive)
                | (IntentShared, SchemaShared)
                | (IntentExclusive, IntentShared)
                | (IntentExclusive, IntentExclusive)
                | (IntentExclusive, SchemaShared)
                | (SchemaShared, Shared)
                | (SchemaShared, IntentShared)
                | (SchemaShared, IntentExclusive)
                | (SchemaShared, SchemaShared)
        )
    }
}

/// Storage-agnostic resource identity used by the lock manager.
///
/// The identifiers are intentionally opaque `u64` values. Zero is rejected by
/// constructors because it is reserved across Andromeda identifier domains as
/// "no id" / "no resource".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LockResource {
    Schema {
        schema_id: u64,
    },
    Table {
        schema_id: u64,
        table_id: u64,
    },
    Page {
        schema_id: u64,
        table_id: u64,
        page_id: u64,
    },
    Row {
        schema_id: u64,
        table_id: u64,
        row_id: u64,
    },
}

impl LockResource {
    pub fn schema(schema_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock schema resource id must not be zero")?;
        Ok(Self::Schema { schema_id })
    }

    pub fn table(schema_id: u64, table_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock table schema id must not be zero")?;
        validate_non_zero(table_id, "lock table resource id must not be zero")?;
        Ok(Self::Table {
            schema_id,
            table_id,
        })
    }

    pub fn page(schema_id: u64, table_id: u64, page_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock page schema id must not be zero")?;
        validate_non_zero(table_id, "lock page table id must not be zero")?;
        validate_non_zero(page_id, "lock page resource id must not be zero")?;
        Ok(Self::Page {
            schema_id,
            table_id,
            page_id,
        })
    }

    pub fn row(schema_id: u64, table_id: u64, row_id: u64) -> AndromedaResult<Self> {
        validate_non_zero(schema_id, "lock row schema id must not be zero")?;
        validate_non_zero(table_id, "lock row table id must not be zero")?;
        validate_non_zero(row_id, "lock row resource id must not be zero")?;
        Ok(Self::Row {
            schema_id,
            table_id,
            row_id,
        })
    }
}

/// Transaction currently holding a lock on a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LockHolder {
    pub tx_id: TransactionId,
    pub mode: LockMode,
}

impl LockHolder {
    pub fn new(tx_id: TransactionId, mode: LockMode) -> AndromedaResult<Self> {
        validate_transaction_id(tx_id)?;
        Ok(Self { tx_id, mode })
    }
}

/// Transaction waiting for a lock on a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LockWaiter {
    pub tx_id: TransactionId,
    pub mode: LockMode,
    pub sequence: u64,
}

impl LockWaiter {
    pub fn new(tx_id: TransactionId, mode: LockMode, sequence: u64) -> AndromedaResult<Self> {
        validate_transaction_id(tx_id)?;
        validate_non_zero(sequence, "lock waiter sequence must not be zero")?;
        Ok(Self {
            tx_id,
            mode,
            sequence,
        })
    }
}

/// In-memory lock queue for one resource.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockEntry {
    pub holders: Vec<LockHolder>,
    pub waiters: VecDeque<LockWaiter>,
}

impl LockEntry {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of a nonblocking acquire attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockAcquireStatus {
    /// The requested lock was recorded as a holder immediately.
    Granted,
    /// The same transaction already holds this exact lock mode, or a V0 mode
    /// treated as covering the requested mode, on the resource.
    ///
    /// No duplicate holder is recorded.
    AlreadyHeld { held_mode: LockMode },
    /// The same transaction held a weaker V0 mode and the holder record was
    /// upgraded in place without creating a duplicate holder.
    Upgraded {
        previous_mode: LockMode,
        new_mode: LockMode,
    },
    /// The request was enqueued without blocking the caller's OS thread.
    Waiting {
        sequence: u64,
        blockers: Vec<LockHolder>,
    },
    /// A same-transaction upgrade could not be applied immediately and was
    /// enqueued behind existing waiters without changing the current holder mode.
    WaitingUpgrade {
        sequence: u64,
        held_mode: LockMode,
        requested_mode: LockMode,
        blockers: Vec<LockHolder>,
    },
}

/// Minimal lock manager.
///
/// C3-LM-001 provides type-safe construction, queue storage, and non-panicking
/// error paths. C3-LM-003 provides nonblocking compatibility decisions and FIFO
/// waiter insertion. C3-LM-004 provides same-transaction re-entry, V0 upgrades,
/// and conservative FIFO fairness for incompatible older waiters. C3-LM-005
/// promotes compatible waiters after release while preserving FIFO order for the
/// first incompatible waiter. Deadlock handling is deliberately deferred to
/// later lock-manager TODOs.
#[derive(Debug)]
pub struct LockManager {
    inner: Mutex<LockManagerInner>,
    next_sequence: AtomicU64,
}

#[derive(Debug, Default)]
struct LockManagerInner {
    entries: HashMap<LockResource, LockEntry>,
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(LockManagerInner::default()),
            next_sequence: AtomicU64::new(0),
        }
    }

    /// Ensure that a resource has an entry in the lock table.
    pub fn ensure_entry(&self, resource: LockResource) -> AndromedaResult<()> {
        validate_lock_resource(resource)?;
        let mut inner = self.lock_inner()?;
        inner.entries.entry(resource).or_insert_with(LockEntry::new);
        Ok(())
    }

    /// Return a cloned snapshot of one resource entry, if present.
    pub fn entry(&self, resource: LockResource) -> AndromedaResult<Option<LockEntry>> {
        validate_lock_resource(resource)?;
        let inner = self.lock_inner()?;
        Ok(inner.entries.get(&resource).cloned())
    }

    /// Try to acquire a lock without blocking an OS thread or assuming an async
    /// runtime.
    ///
    /// The V0 policy grants immediately when all current holders from other
    /// transactions are compatible with `mode` and the grant would not bypass an
    /// older incompatible waiter. Same-mode same-transaction re-entry returns
    /// [`LockAcquireStatus::AlreadyHeld`] without creating duplicate holders.
    /// Narrow V0 conversions are supported without blocking: `Shared` may
    /// upgrade to `Exclusive`, and `IntentShared` may upgrade to
    /// `IntentExclusive`, when other holders and older waiters are compatible
    /// with the requested mode. If an upgrade cannot be applied immediately, the
    /// upgrade request is appended to the FIFO waiter queue and the current
    /// holder is left unchanged. Deadlock detection and release promotion are not
    /// performed by this method.
    pub fn acquire(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireStatus> {
        validate_transaction_id(tx_id)?;
        validate_lock_resource(resource)?;
        let mut inner = self.lock_inner()?;
        let entry = inner.entries.entry(resource).or_insert_with(LockEntry::new);

        if let Some(holder_index) = entry
            .holders
            .iter()
            .position(|holder| holder.tx_id == tx_id)
        {
            let held_mode = entry.holders[holder_index].mode;
            if held_mode == mode || held_mode_covers_requested(held_mode, mode) {
                return Ok(LockAcquireStatus::AlreadyHeld { held_mode });
            }

            if !is_v0_upgrade(held_mode, mode) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "lock conversion between different modes is not supported by V0",
                ));
            }

            let blockers: Vec<LockHolder> = entry
                .holders
                .iter()
                .copied()
                .filter(|holder| holder.tx_id != tx_id && !holder.mode.is_compatible_with(mode))
                .collect();

            let has_older_incompatible_waiter = entry
                .waiters
                .iter()
                .any(|waiter| !waiter.mode.is_compatible_with(mode));

            if blockers.is_empty() && !has_older_incompatible_waiter {
                entry.holders[holder_index].mode = mode;
                return Ok(LockAcquireStatus::Upgraded {
                    previous_mode: held_mode,
                    new_mode: mode,
                });
            }

            if let Some(existing_waiter) = entry
                .waiters
                .iter()
                .find(|waiter| waiter.tx_id == tx_id && waiter.mode == mode)
            {
                return Ok(LockAcquireStatus::WaitingUpgrade {
                    sequence: existing_waiter.sequence,
                    held_mode,
                    requested_mode: mode,
                    blockers,
                });
            }

            let sequence = self.allocate_sequence()?;
            let waiter = LockWaiter::new(tx_id, mode, sequence)?;
            entry.waiters.push_back(waiter);

            return Ok(LockAcquireStatus::WaitingUpgrade {
                sequence,
                held_mode,
                requested_mode: mode,
                blockers,
            });
        }

        if let Some(existing_waiter) = entry
            .waiters
            .iter()
            .find(|waiter| waiter.tx_id == tx_id && waiter.mode == mode)
        {
            let blockers: Vec<LockHolder> = entry
                .holders
                .iter()
                .copied()
                .filter(|holder| !holder.mode.is_compatible_with(mode))
                .collect();

            return Ok(LockAcquireStatus::Waiting {
                sequence: existing_waiter.sequence,
                blockers,
            });
        }

        let blockers: Vec<LockHolder> = entry
            .holders
            .iter()
            .copied()
            .filter(|holder| !holder.mode.is_compatible_with(mode))
            .collect();

        let has_older_incompatible_waiter = entry
            .waiters
            .iter()
            .any(|waiter| !waiter.mode.is_compatible_with(mode));

        if blockers.is_empty() && !has_older_incompatible_waiter {
            entry.holders.push(LockHolder { tx_id, mode });
            return Ok(LockAcquireStatus::Granted);
        }

        let sequence = self.allocate_sequence()?;
        let waiter = LockWaiter::new(tx_id, mode, sequence)?;
        entry.waiters.push_back(waiter);

        Ok(LockAcquireStatus::Waiting { sequence, blockers })
    }

    /// Queue a waiter without evaluating lock compatibility.
    ///
    /// This is a skeleton helper for tests and upcoming acquire work. It
    /// assigns a monotonic, non-zero sequence number and stores the waiter.
    pub fn enqueue_waiter(
        &self,
        resource: LockResource,
        tx_id: TransactionId,
        mode: LockMode,
    ) -> AndromedaResult<u64> {
        validate_transaction_id(tx_id)?;
        validate_lock_resource(resource)?;
        let mut inner = self.lock_inner()?;
        let sequence = self.allocate_sequence()?;
        let waiter = LockWaiter::new(tx_id, mode, sequence)?;
        inner
            .entries
            .entry(resource)
            .or_insert_with(LockEntry::new)
            .waiters
            .push_back(waiter);
        Ok(sequence)
    }

    /// Record a holder without evaluating lock compatibility.
    ///
    /// This exists only as a compile-safe skeleton hook. It must not be used as
    /// a substitute for the final acquire path because it performs no conflict
    /// checks.
    pub fn record_holder(
        &self,
        resource: LockResource,
        tx_id: TransactionId,
        mode: LockMode,
    ) -> AndromedaResult<()> {
        validate_lock_resource(resource)?;
        let holder = LockHolder::new(tx_id, mode)?;
        let mut inner = self.lock_inner()?;
        inner
            .entries
            .entry(resource)
            .or_insert_with(LockEntry::new)
            .holders
            .push(holder);
        Ok(())
    }

    /// Remove all holder and waiter records for `tx_id` on `resource`.
    ///
    /// After removal, compatible waiters are promoted from the front of the FIFO
    /// queue until the queue is empty or the next waiter is incompatible with the
    /// current holder set. Waiting upgrades update the transaction's existing
    /// holder in place and never create a duplicate holder.
    pub fn release(&self, tx_id: TransactionId, resource: LockResource) -> AndromedaResult<bool> {
        validate_transaction_id(tx_id)?;
        validate_lock_resource(resource)?;
        let mut inner = self.lock_inner()?;
        let (removed_any, remove_empty_entry) = {
            let Some(entry) = inner.entries.get_mut(&resource) else {
                return Ok(false);
            };

            let holder_count_before = entry.holders.len();
            let waiter_count_before = entry.waiters.len();
            entry.holders.retain(|holder| holder.tx_id != tx_id);
            entry.waiters.retain(|waiter| waiter.tx_id != tx_id);

            let removed_any = holder_count_before != entry.holders.len()
                || waiter_count_before != entry.waiters.len();

            if removed_any {
                promote_compatible_waiters(entry);
            }

            (
                removed_any,
                entry.holders.is_empty() && entry.waiters.is_empty(),
            )
        };

        if remove_empty_entry {
            inner.entries.remove(&resource);
        }

        Ok(removed_any)
    }

    /// Number of resource entries currently tracked.
    pub fn entry_count(&self) -> AndromedaResult<usize> {
        Ok(self.lock_inner()?.entries.len())
    }

    fn allocate_sequence(&self) -> AndromedaResult<u64> {
        self.next_sequence
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| {
                AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "lock waiter sequence counter overflowed",
                )
            })
    }

    fn lock_inner(&self) -> AndromedaResult<MutexGuard<'_, LockManagerInner>> {
        self.inner.lock().map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "lock manager mutex was poisoned",
            )
        })
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_transaction_id(tx_id: TransactionId) -> AndromedaResult<()> {
    if tx_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "lock transaction id must not be zero",
        ));
    }

    Ok(())
}

fn validate_lock_resource(resource: LockResource) -> AndromedaResult<()> {
    match resource {
        LockResource::Schema { schema_id } => {
            validate_non_zero(schema_id, "lock schema resource id must not be zero")?;
        }
        LockResource::Table {
            schema_id,
            table_id,
        } => {
            validate_non_zero(schema_id, "lock table schema id must not be zero")?;
            validate_non_zero(table_id, "lock table resource id must not be zero")?;
        }
        LockResource::Page {
            schema_id,
            table_id,
            page_id,
        } => {
            validate_non_zero(schema_id, "lock page schema id must not be zero")?;
            validate_non_zero(table_id, "lock page table id must not be zero")?;
            validate_non_zero(page_id, "lock page resource id must not be zero")?;
        }
        LockResource::Row {
            schema_id,
            table_id,
            row_id,
        } => {
            validate_non_zero(schema_id, "lock row schema id must not be zero")?;
            validate_non_zero(table_id, "lock row table id must not be zero")?;
            validate_non_zero(row_id, "lock row resource id must not be zero")?;
        }
    }

    Ok(())
}

fn held_mode_covers_requested(held_mode: LockMode, requested_mode: LockMode) -> bool {
    matches!(
        (held_mode, requested_mode),
        (LockMode::Exclusive, LockMode::Shared)
            | (LockMode::Exclusive, LockMode::IntentShared)
            | (LockMode::Exclusive, LockMode::IntentExclusive)
            | (LockMode::IntentExclusive, LockMode::IntentShared)
            | (LockMode::SchemaExclusive, LockMode::SchemaShared)
    )
}

fn is_v0_upgrade(held_mode: LockMode, requested_mode: LockMode) -> bool {
    matches!(
        (held_mode, requested_mode),
        (LockMode::Shared, LockMode::Exclusive)
            | (LockMode::IntentShared, LockMode::IntentExclusive)
            | (LockMode::SchemaShared, LockMode::SchemaExclusive)
    )
}

fn promote_compatible_waiters(entry: &mut LockEntry) {
    while let Some(waiter) = entry.waiters.front().copied() {
        if !waiter_is_compatible_with_holders(entry, waiter) {
            break;
        }

        entry.waiters.pop_front();
        promote_waiter_to_holder(entry, waiter);
    }
}

fn waiter_is_compatible_with_holders(entry: &LockEntry, waiter: LockWaiter) -> bool {
    entry
        .holders
        .iter()
        .filter(|holder| holder.tx_id != waiter.tx_id)
        .all(|holder| holder.mode.is_compatible_with(waiter.mode))
}

fn promote_waiter_to_holder(entry: &mut LockEntry, waiter: LockWaiter) {
    if let Some(holder) = entry
        .holders
        .iter_mut()
        .find(|holder| holder.tx_id == waiter.tx_id)
    {
        if holder.mode != waiter.mode && !held_mode_covers_requested(holder.mode, waiter.mode) {
            holder.mode = waiter.mode;
        }
        return;
    }

    entry.holders.push(LockHolder {
        tx_id: waiter.tx_id,
        mode: waiter.mode,
    });
}

fn validate_non_zero(value: u64, message: &'static str) -> AndromedaResult<()> {
    if value == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_LOCK_MODES: [LockMode; 6] = [
        LockMode::Shared,
        LockMode::Exclusive,
        LockMode::IntentShared,
        LockMode::IntentExclusive,
        LockMode::SchemaShared,
        LockMode::SchemaExclusive,
    ];

    #[test]
    fn resource_constructors_reject_zero_components() {
        assert!(LockResource::schema(0).is_err());
        assert!(LockResource::table(1, 0).is_err());
        assert!(LockResource::page(1, 1, 0).is_err());
        assert!(LockResource::row(1, 1, 0).is_err());
    }

    #[test]
    fn lock_types_construct_with_valid_inputs() {
        let tx_id = TransactionId::new(1);
        let holder = LockHolder::new(tx_id, LockMode::Shared).unwrap();
        let waiter = LockWaiter::new(tx_id, LockMode::Exclusive, 7).unwrap();

        assert_eq!(holder.tx_id, tx_id);
        assert_eq!(holder.mode, LockMode::Shared);
        assert_eq!(waiter.sequence, 7);
    }

    #[test]
    fn lock_mode_v0_compatibility_matrix_is_exhaustive() {
        let expected = [
            ((LockMode::Shared, LockMode::Shared), true),
            ((LockMode::Shared, LockMode::Exclusive), false),
            ((LockMode::Shared, LockMode::IntentShared), true),
            ((LockMode::Shared, LockMode::IntentExclusive), false),
            ((LockMode::Shared, LockMode::SchemaShared), true),
            ((LockMode::Shared, LockMode::SchemaExclusive), false),
            ((LockMode::Exclusive, LockMode::Shared), false),
            ((LockMode::Exclusive, LockMode::Exclusive), false),
            ((LockMode::Exclusive, LockMode::IntentShared), false),
            ((LockMode::Exclusive, LockMode::IntentExclusive), false),
            ((LockMode::Exclusive, LockMode::SchemaShared), false),
            ((LockMode::Exclusive, LockMode::SchemaExclusive), false),
            ((LockMode::IntentShared, LockMode::Shared), true),
            ((LockMode::IntentShared, LockMode::Exclusive), false),
            ((LockMode::IntentShared, LockMode::IntentShared), true),
            ((LockMode::IntentShared, LockMode::IntentExclusive), true),
            ((LockMode::IntentShared, LockMode::SchemaShared), true),
            ((LockMode::IntentShared, LockMode::SchemaExclusive), false),
            ((LockMode::IntentExclusive, LockMode::Shared), false),
            ((LockMode::IntentExclusive, LockMode::Exclusive), false),
            ((LockMode::IntentExclusive, LockMode::IntentShared), true),
            ((LockMode::IntentExclusive, LockMode::IntentExclusive), true),
            ((LockMode::IntentExclusive, LockMode::SchemaShared), true),
            (
                (LockMode::IntentExclusive, LockMode::SchemaExclusive),
                false,
            ),
            ((LockMode::SchemaShared, LockMode::Shared), true),
            ((LockMode::SchemaShared, LockMode::Exclusive), false),
            ((LockMode::SchemaShared, LockMode::IntentShared), true),
            ((LockMode::SchemaShared, LockMode::IntentExclusive), true),
            ((LockMode::SchemaShared, LockMode::SchemaShared), true),
            ((LockMode::SchemaShared, LockMode::SchemaExclusive), false),
            ((LockMode::SchemaExclusive, LockMode::Shared), false),
            ((LockMode::SchemaExclusive, LockMode::Exclusive), false),
            ((LockMode::SchemaExclusive, LockMode::IntentShared), false),
            (
                (LockMode::SchemaExclusive, LockMode::IntentExclusive),
                false,
            ),
            ((LockMode::SchemaExclusive, LockMode::SchemaShared), false),
            (
                (LockMode::SchemaExclusive, LockMode::SchemaExclusive),
                false,
            ),
        ];

        let mut observed_pairs = 0;
        for existing in ALL_LOCK_MODES {
            for requested in ALL_LOCK_MODES {
                let (_, compatible) = expected
                    .iter()
                    .find(|((expected_existing, expected_requested), _)| {
                        *expected_existing == existing && *expected_requested == requested
                    })
                    .expect("every lock-mode pair must be listed");

                assert_eq!(
                    existing.is_compatible_with(requested),
                    *compatible,
                    "existing={existing:?}, requested={requested:?}"
                );
                observed_pairs += 1;
            }
        }

        assert_eq!(observed_pairs, 36);
        assert_eq!(expected.len(), 36);
    }

    #[test]
    fn lock_mode_v0_compatibility_matrix_is_symmetric() {
        for existing in ALL_LOCK_MODES {
            for requested in ALL_LOCK_MODES {
                assert_eq!(
                    existing.is_compatible_with(requested),
                    requested.is_compatible_with(existing),
                    "existing={existing:?}, requested={requested:?}"
                );
            }
        }
    }

    #[test]
    fn manager_tracks_entries_and_waiters() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let tx_id = TransactionId::new(11);

        manager.ensure_entry(resource).unwrap();
        assert_eq!(manager.entry_count().unwrap(), 1);

        let sequence = manager
            .enqueue_waiter(resource, tx_id, LockMode::Exclusive)
            .unwrap();
        assert_eq!(sequence, 1);

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 0);
        assert_eq!(entry.waiters.len(), 1);
        assert_eq!(entry.waiters[0].tx_id, tx_id);
    }

    #[test]
    fn acquire_grants_immediately_when_compatible() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        let first = manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap();
        let second = manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap();

        assert_eq!(first, LockAcquireStatus::Granted);
        assert_eq!(second, LockAcquireStatus::Granted);

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(
            entry.holders,
            vec![
                LockHolder {
                    tx_id: TransactionId::new(1),
                    mode: LockMode::Shared,
                },
                LockHolder {
                    tx_id: TransactionId::new(2),
                    mode: LockMode::Shared,
                },
            ]
        );
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn acquire_waits_with_blockers_when_incompatible() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        let status = manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap();

        assert_eq!(
            status,
            LockAcquireStatus::Waiting {
                sequence: 1,
                blockers: vec![LockHolder {
                    tx_id: TransactionId::new(1),
                    mode: LockMode::Exclusive,
                }],
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 1);
        assert_eq!(entry.waiters.len(), 1);
        assert_eq!(entry.waiters[0].tx_id, TransactionId::new(2));
        assert_eq!(entry.waiters[0].sequence, 1);
    }

    #[test]
    fn acquire_reports_same_transaction_already_held_without_duplicate_holder() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let tx_id = TransactionId::new(7);

        assert_eq!(
            manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
            LockAcquireStatus::AlreadyHeld {
                held_mode: LockMode::Shared,
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 1);
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn acquire_upgrades_shared_to_exclusive_in_place_when_unblocked() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let tx_id = TransactionId::new(7);

        assert_eq!(
            manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
            LockAcquireStatus::Granted
        );

        assert_eq!(
            manager
                .acquire(tx_id, resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Upgraded {
                previous_mode: LockMode::Shared,
                new_mode: LockMode::Exclusive,
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 1);
        assert_eq!(entry.holders[0].tx_id, tx_id);
        assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn acquire_waits_upgrade_when_other_holders_block_shared_to_exclusive() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let upgrading_tx = TransactionId::new(7);
        let blocking_tx = TransactionId::new(8);

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(blocking_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::WaitingUpgrade {
                sequence: 1,
                held_mode: LockMode::Shared,
                requested_mode: LockMode::Exclusive,
                blockers: vec![LockHolder {
                    tx_id: blocking_tx,
                    mode: LockMode::Shared,
                }],
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 2);
        assert_eq!(entry.holders[0].mode, LockMode::Shared);
        assert_eq!(entry.waiters.len(), 1);
        assert_eq!(entry.waiters[0].tx_id, upgrading_tx);
        assert_eq!(entry.waiters[0].mode, LockMode::Exclusive);
        assert_eq!(entry.waiters[0].sequence, 1);
    }

    #[test]
    fn acquire_upgrades_intent_shared_to_intent_exclusive_when_compatible() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();
        let upgrading_tx = TransactionId::new(10);
        let compatible_tx = TransactionId::new(11);

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::IntentShared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(compatible_tx, resource, LockMode::IntentShared)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::IntentExclusive)
                .unwrap(),
            LockAcquireStatus::Upgraded {
                previous_mode: LockMode::IntentShared,
                new_mode: LockMode::IntentExclusive,
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 2);
        assert_eq!(
            entry
                .holders
                .iter()
                .find(|holder| holder.tx_id == upgrading_tx)
                .unwrap()
                .mode,
            LockMode::IntentExclusive
        );
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn acquire_does_not_bypass_older_incompatible_waiter() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Waiting {
                sequence: 1,
                blockers: vec![LockHolder {
                    tx_id: TransactionId::new(1),
                    mode: LockMode::Shared,
                }],
            }
        );

        assert_eq!(
            manager
                .acquire(TransactionId::new(3), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting {
                sequence: 2,
                blockers: vec![],
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 1);
        assert_eq!(entry.waiters.len(), 2);
        assert_eq!(entry.waiters[0].tx_id, TransactionId::new(2));
        assert_eq!(entry.waiters[0].sequence, 1);
        assert_eq!(entry.waiters[1].tx_id, TransactionId::new(3));
        assert_eq!(entry.waiters[1].sequence, 2);
    }

    #[test]
    fn acquire_reentrant_waiting_upgrade_does_not_duplicate_waiter_or_holder() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let upgrading_tx = TransactionId::new(7);
        let blocking_tx = TransactionId::new(8);

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(blocking_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        let first = manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap();
        let second = manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap();

        assert!(matches!(
            first,
            LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
        ));
        assert!(matches!(
            second,
            LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
        ));

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 2);
        assert_eq!(
            entry
                .holders
                .iter()
                .filter(|holder| holder.tx_id == upgrading_tx)
                .count(),
            1
        );
        assert_eq!(entry.waiters.len(), 1);
        assert_eq!(entry.waiters[0].tx_id, upgrading_tx);
        assert_eq!(entry.waiters[0].sequence, 1);
    }

    #[test]
    fn acquire_stronger_held_mode_reports_already_held_without_duplicate() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let tx_id = TransactionId::new(7);

        assert_eq!(
            manager
                .acquire(tx_id, resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
            LockAcquireStatus::AlreadyHeld {
                held_mode: LockMode::Exclusive,
            }
        );

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 1);
        assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn acquire_waiter_sequences_are_fifo_and_deterministic() {
        let manager = LockManager::new();
        let resource = LockResource::schema(1).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::SchemaExclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        let first_wait = manager
            .acquire(TransactionId::new(2), resource, LockMode::SchemaShared)
            .unwrap();
        let second_wait = manager
            .acquire(TransactionId::new(3), resource, LockMode::Shared)
            .unwrap();

        assert!(matches!(
            first_wait,
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));
        assert!(matches!(
            second_wait,
            LockAcquireStatus::Waiting { sequence: 2, .. }
        ));

        let entry = manager.entry(resource).unwrap().unwrap();
        let waiters: Vec<(TransactionId, u64)> = entry
            .waiters
            .iter()
            .map(|waiter| (waiter.tx_id, waiter.sequence))
            .collect();
        assert_eq!(
            waiters,
            vec![(TransactionId::new(2), 1), (TransactionId::new(3), 2)]
        );
    }

    #[test]
    fn release_removes_holder_and_keeps_non_empty_entry() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        assert!(manager.release(TransactionId::new(1), resource).unwrap());

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(
            entry.holders,
            vec![LockHolder {
                tx_id: TransactionId::new(2),
                mode: LockMode::Shared,
            }]
        );
        assert!(entry.waiters.is_empty());
        assert_eq!(manager.entry_count().unwrap(), 1);
    }

    #[test]
    fn release_promotes_compatible_shared_waiters_in_fifo_order() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));
        assert!(matches!(
            manager
                .acquire(TransactionId::new(3), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 2, .. }
        ));

        assert!(manager.release(TransactionId::new(1), resource).unwrap());

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(
            entry.holders,
            vec![
                LockHolder {
                    tx_id: TransactionId::new(2),
                    mode: LockMode::Shared,
                },
                LockHolder {
                    tx_id: TransactionId::new(3),
                    mode: LockMode::Shared,
                },
            ]
        );
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn release_promotes_waiting_exclusive_when_compatible_after_holder_removal() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));

        assert!(manager.release(TransactionId::new(1), resource).unwrap());

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(
            entry.holders,
            vec![LockHolder {
                tx_id: TransactionId::new(2),
                mode: LockMode::Exclusive,
            }]
        );
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn release_preserves_fifo_when_incompatible_waiter_blocks_later_compatible_waiter() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(TransactionId::new(3), resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));
        assert!(matches!(
            manager
                .acquire(TransactionId::new(4), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 2, .. }
        ));

        assert!(manager.release(TransactionId::new(1), resource).unwrap());

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(
            entry.holders,
            vec![LockHolder {
                tx_id: TransactionId::new(2),
                mode: LockMode::Shared,
            }]
        );
        let waiters: Vec<(TransactionId, LockMode, u64)> = entry
            .waiters
            .iter()
            .map(|waiter| (waiter.tx_id, waiter.mode, waiter.sequence))
            .collect();
        assert_eq!(
            waiters,
            vec![
                (TransactionId::new(3), LockMode::Exclusive, 1),
                (TransactionId::new(4), LockMode::Shared, 2),
            ]
        );
    }

    #[test]
    fn release_promotes_waiting_upgrade_in_place() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let upgrading_tx = TransactionId::new(7);
        let blocking_tx = TransactionId::new(8);

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(blocking_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
        ));

        assert!(manager.release(blocking_tx, resource).unwrap());

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(entry.holders.len(), 1);
        assert_eq!(entry.holders[0].tx_id, upgrading_tx);
        assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn release_removes_empty_entry_after_final_holder_release() {
        let manager = LockManager::new();
        let resource = LockResource::schema(1).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::SchemaShared)
                .unwrap(),
            LockAcquireStatus::Granted
        );

        assert!(manager.release(TransactionId::new(1), resource).unwrap());
        assert_eq!(manager.entry(resource).unwrap(), None);
        assert_eq!(manager.entry_count().unwrap(), 0);
    }

    #[test]
    fn release_promotion_does_not_create_duplicate_holder_for_waiting_upgrade() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();
        let upgrading_tx = TransactionId::new(10);
        let blocking_tx = TransactionId::new(11);

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::IntentShared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(blocking_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(upgrading_tx, resource, LockMode::IntentExclusive)
                .unwrap(),
            LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
        ));

        assert!(manager.release(blocking_tx, resource).unwrap());

        let entry = manager.entry(resource).unwrap().unwrap();
        assert_eq!(
            entry
                .holders
                .iter()
                .filter(|holder| holder.tx_id == upgrading_tx)
                .count(),
            1
        );
        assert_eq!(entry.holders[0].mode, LockMode::IntentExclusive);
        assert!(entry.waiters.is_empty());
    }

    #[test]
    fn acquire_rejects_invalid_transaction_id() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        let zero_tx = manager.acquire(TransactionId::new(0), resource, LockMode::Shared);
        assert_eq!(zero_tx.unwrap_err().kind(), AndromedaErrorKind::Transaction);
        assert_eq!(manager.entry_count().unwrap(), 0);
    }

    #[test]
    fn acquire_rejects_invalid_resource_id() {
        let manager = LockManager::new();
        let resource = LockResource::Table {
            schema_id: 1,
            table_id: 0,
        };

        let invalid_resource = manager.acquire(TransactionId::new(1), resource, LockMode::Shared);
        assert_eq!(
            invalid_resource.unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
        assert_eq!(manager.entry_count().unwrap(), 0);
    }

    #[test]
    fn acquire_does_not_panic_on_grant_wait_or_reentrant_paths() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        let result = std::panic::catch_unwind(|| {
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap();
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap();
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
                .unwrap();
        });

        assert!(result.is_ok());
    }
}
