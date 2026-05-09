use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_types::TransactionId;

use super::entry::{LockEntry, LockHolder, LockWaiter, promote_compatible_waiters};
use super::evidence::{
    LockAcquireEvidence, LockAcquireStatus, LockDecisionEvidence, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence,
};
use super::mode::{LockMode, held_mode_covers_requested, is_v0_upgrade};
use super::resource::{self, LockResource};

/// Minimal lock manager.
///
/// C3-LM-001 provides type-safe construction, queue storage, and non-panicking
/// error paths. C3-LM-003 provides nonblocking compatibility decisions and FIFO
/// waiter insertion. C3-LM-004 provides same-transaction re-entry, V0 upgrades,
/// and conservative FIFO fairness for incompatible older waiters. C3-LM-005
/// promotes compatible waiters after release while preserving FIFO order for the
/// first incompatible waiter. C3-LM-006 provides transaction-end `release_all`
/// cleanup after durable commit/rollback integration points. Deadlock handling
/// is deliberately deferred to a later lock-manager milestone.
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

    /// Return a deterministic cloned snapshot of the full lock table.
    ///
    /// The snapshot exposes owned copies only: callers can inspect holders and
    /// waiters for evidence-building, including deadlock wait-for graph
    /// derivation, without receiving mutable access to the lock manager's
    /// internal table. Resource ordering is stable across runs even though the
    /// backing table uses a hash map.
    pub fn snapshot(&self) -> AndromedaResult<Vec<(LockResource, LockEntry)>> {
        let inner = self.lock_inner()?;
        let mut entries: Vec<(LockResource, LockEntry)> = inner
            .entries
            .iter()
            .map(|(resource, entry)| (*resource, entry.clone()))
            .collect();
        entries.sort_by_key(|(resource, _)| *resource);
        Ok(entries)
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

    /// Acquire with transaction-local trace projection for critical waits.
    ///
    /// This is a non-breaking hook around [`Self::acquire`]; the lock-table
    /// mutation and returned status remain identical to the existing API.
    pub fn acquire_with_evidence(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireEvidence> {
        let status = self.acquire(tx_id, resource, mode)?;
        let evidence = status.critical_wait_evidence(tx_id, resource, mode);
        Ok(LockAcquireEvidence { status, evidence })
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
        Ok(self.release_with_evidence(tx_id, resource)?.released_any)
    }

    /// Release one resource with transaction-local promotion traces.
    ///
    /// The evidence reports FIFO waiter promotions caused by this release only.
    /// It does not imply abort, rollback, commit, WAL, or durability state.
    pub fn release_with_evidence(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
    ) -> AndromedaResult<LockReleaseEvidence> {
        validate_transaction_id(tx_id)?;
        validate_lock_resource(resource)?;
        let mut inner = self.lock_inner()?;
        let (released_any, remove_empty_entry, evidence) = {
            let Some(entry) = inner.entries.get_mut(&resource) else {
                return Ok(LockReleaseEvidence {
                    released_any: false,
                    evidence: Vec::new(),
                });
            };

            let holder_count_before = entry.holders.len();
            let waiter_count_before = entry.waiters.len();
            entry.holders.retain(|holder| holder.tx_id != tx_id);
            entry.waiters.retain(|waiter| waiter.tx_id != tx_id);

            let removed_any = holder_count_before != entry.holders.len()
                || waiter_count_before != entry.waiters.len();

            let mut evidence = Vec::new();
            if removed_any {
                evidence.extend(
                    promote_compatible_waiters(entry)
                        .into_iter()
                        .map(|waiter| LockDecisionEvidence::promotion(resource, waiter)),
                );
            }

            (
                removed_any,
                entry.holders.is_empty() && entry.waiters.is_empty(),
                evidence,
            )
        };

        if remove_empty_entry {
            inner.entries.remove(&resource);
        }

        Ok(LockReleaseEvidence {
            released_any,
            evidence,
        })
    }

    /// Remove all holder and waiter records for `tx_id` across all resources.
    ///
    /// This is the strict 2PL terminal cleanup primitive. It is valid only at a
    /// transaction-end boundary after the caller has durable evidence for either
    /// a commit decision or a rollback decision. Calling this method must not be
    /// interpreted as making commit/rollback durable, and it must not be used
    /// for early release while a transaction can still perform work.
    ///
    /// For each affected resource, compatible waiters are promoted from the
    /// front of that resource's FIFO queue after the transaction's records are
    /// removed. Empty lock-table entries are removed. The returned summary is
    /// deterministic count evidence and does not depend on hash-map iteration
    /// order.
    pub fn release_all(&self, tx_id: TransactionId) -> AndromedaResult<LockReleaseAllSummary> {
        Ok(self.release_all_with_evidence(tx_id)?.summary)
    }

    /// Remove all lock records with transaction-local cleanup and promotion
    /// traces.
    ///
    /// The first evidence item is always the release-all summary projection.
    /// Subsequent items, if any, are FIFO waiter promotion traces.
    pub fn release_all_with_evidence(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<LockReleaseAllEvidence> {
        validate_transaction_id(tx_id)?;
        let mut inner = self.lock_inner()?;
        let mut summary = LockReleaseAllSummary::default();
        let mut evidence = Vec::new();
        let mut empty_resources = Vec::new();
        let mut resources: Vec<LockResource> = inner.entries.keys().copied().collect();
        resources.sort();

        for resource in resources {
            let Some(entry) = inner.entries.get_mut(&resource) else {
                continue;
            };
            let holder_count_before = entry.holders.len();
            let waiter_count_before = entry.waiters.len();

            entry.holders.retain(|holder| holder.tx_id != tx_id);
            entry.waiters.retain(|waiter| waiter.tx_id != tx_id);

            let released_holder_count = holder_count_before - entry.holders.len();
            let removed_waiter_count = waiter_count_before - entry.waiters.len();
            if released_holder_count == 0 && removed_waiter_count == 0 {
                continue;
            }

            summary.affected_resource_count += 1;
            summary.released_holder_count += released_holder_count;
            summary.removed_waiter_count += removed_waiter_count;

            evidence.extend(
                promote_compatible_waiters(entry)
                    .into_iter()
                    .map(|waiter| LockDecisionEvidence::promotion(resource, waiter)),
            );

            if entry.holders.is_empty() && entry.waiters.is_empty() {
                empty_resources.push(resource);
            }
        }

        for resource in empty_resources {
            inner.entries.remove(&resource);
        }

        evidence.insert(0, summary.to_trace(tx_id));

        Ok(LockReleaseAllEvidence { summary, evidence })
    }

    /// Number of resource entries currently tracked.
    pub fn entry_count(&self) -> AndromedaResult<usize> {
        Ok(self.lock_inner()?.entries.len())
    }

    fn allocate_sequence(&self) -> AndromedaResult<u64> {
        loop {
            let current = self.next_sequence.load(Ordering::SeqCst);
            let next = current.checked_add(1).ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "lock waiter sequence counter overflowed",
                )
            })?;

            if self
                .next_sequence
                .compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(next);
            }
        }
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

pub(super) fn validate_transaction_id(tx_id: TransactionId) -> AndromedaResult<()> {
    if tx_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "lock transaction id must not be zero",
        ));
    }

    Ok(())
}

fn validate_lock_resource(resource: LockResource) -> AndromedaResult<()> {
    resource::validate_lock_resource(resource)
}

pub(super) fn validate_non_zero(value: u64, message: &'static str) -> AndromedaResult<()> {
    if value == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}
