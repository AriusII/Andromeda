use std::collections::VecDeque;

use andromeda_core::{AndromedaResult, TransactionId};

use super::mode::held_mode_covers_requested;
use super::{LockMode, validate_non_zero, validate_transaction_id};

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

pub(super) fn promote_compatible_waiters(entry: &mut LockEntry) -> Vec<LockWaiter> {
    let mut promoted = Vec::new();
    while let Some(waiter) = entry.waiters.front().copied() {
        if !waiter_is_compatible_with_holders(entry, waiter) {
            break;
        }

        entry.waiters.pop_front();
        promote_waiter_to_holder(entry, waiter);
        promoted.push(waiter);
    }
    promoted
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
