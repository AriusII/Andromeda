use std::collections::{BTreeMap, BTreeSet};

use andromeda_core::TransactionId;

use crate::lock_manager::{LockEntry, LockManager, LockResource};

use super::{DeadlockResult, validate_not_self_edge, validate_transaction_id};

/// Deterministic wait-for graph.
///
/// Each edge is `waiting_tx -> blocking_tx`. Ordered storage keeps snapshots and
/// tests reproducible across platforms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaitForGraph {
    edges: BTreeMap<TransactionId, BTreeSet<TransactionId>>,
}

impl WaitForGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a wait-for graph from an immutable lock-table snapshot.
    ///
    /// The snapshot is consumed as owned cloned entries, not as references into
    /// the lock manager. For each waiter, the derived graph contains an edge to
    /// every holder whose mode conflicts with the waiter's requested mode.
    /// Waiting lock upgrades are represented by a waiter for a transaction that
    /// may already be in the holder list; that same-transaction holder is skipped
    /// to keep the graph free of self-edges while preserving edges to other
    /// incompatible holders.
    pub fn from_lock_table_snapshot<I>(snapshot: I) -> DeadlockResult<Self>
    where
        I: IntoIterator<Item = (LockResource, LockEntry)>,
    {
        let mut graph = Self::new();

        for (_resource, entry) in snapshot {
            let LockEntry { holders, waiters } = entry;

            for waiter in waiters {
                validate_transaction_id(waiter.tx_id)?;

                for holder in &holders {
                    validate_transaction_id(holder.tx_id)?;

                    if holder.tx_id == waiter.tx_id {
                        continue;
                    }

                    if !holder.mode.is_compatible_with(waiter.mode) {
                        graph.insert_edge(waiter.tx_id, holder.tx_id)?;
                    }
                }
            }
        }

        Ok(graph)
    }

    /// Build a wait-for graph from the lock manager's deterministic cloned
    /// snapshot without exposing mutable lock-table internals.
    pub fn from_lock_manager(lock_manager: &LockManager) -> DeadlockResult<Self> {
        Self::from_lock_table_snapshot(lock_manager.snapshot()?)
    }

    /// Insert a wait-for edge.
    ///
    /// Returns `true` when the graph changed and `false` when the edge already
    /// existed.
    pub fn insert_edge(
        &mut self,
        waiting_tx: TransactionId,
        blocking_tx: TransactionId,
    ) -> DeadlockResult<bool> {
        validate_transaction_id(waiting_tx)?;
        validate_transaction_id(blocking_tx)?;
        validate_not_self_edge(waiting_tx, blocking_tx)?;

        Ok(self
            .edges
            .entry(waiting_tx)
            .or_default()
            .insert(blocking_tx))
    }

    /// Remove one wait-for edge.
    ///
    /// Returns `true` when an edge was removed.
    pub fn remove_edge(
        &mut self,
        waiting_tx: TransactionId,
        blocking_tx: TransactionId,
    ) -> DeadlockResult<bool> {
        validate_transaction_id(waiting_tx)?;
        validate_transaction_id(blocking_tx)?;
        validate_not_self_edge(waiting_tx, blocking_tx)?;

        let Some(blockers) = self.edges.get_mut(&waiting_tx) else {
            return Ok(false);
        };

        let removed = blockers.remove(&blocking_tx);
        if blockers.is_empty() {
            self.edges.remove(&waiting_tx);
        }

        Ok(removed)
    }

    /// Remove all outgoing and incoming edges for one transaction.
    ///
    /// Returns `true` when the graph changed.
    pub fn remove_transaction(&mut self, tx_id: TransactionId) -> DeadlockResult<bool> {
        validate_transaction_id(tx_id)?;

        let removed_outgoing = self.edges.remove(&tx_id).is_some();
        let mut removed_incoming = false;

        self.edges.retain(|_, blockers| {
            if blockers.remove(&tx_id) {
                removed_incoming = true;
            }
            !blockers.is_empty()
        });

        Ok(removed_outgoing || removed_incoming)
    }

    /// Return blockers for a waiting transaction in deterministic order.
    pub fn blockers_for(&self, waiting_tx: TransactionId) -> DeadlockResult<Vec<TransactionId>> {
        validate_transaction_id(waiting_tx)?;
        Ok(self
            .edges
            .get(&waiting_tx)
            .map(|blockers| blockers.iter().copied().collect())
            .unwrap_or_default())
    }

    /// Return all edges as sorted `(waiting_tx, blocking_tx)` pairs.
    pub fn edges(&self) -> Vec<(TransactionId, TransactionId)> {
        self.edges
            .iter()
            .flat_map(|(waiting_tx, blockers)| {
                blockers
                    .iter()
                    .map(move |blocking_tx| (*waiting_tx, *blocking_tx))
            })
            .collect()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.values().map(BTreeSet::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    pub(super) fn transaction_ids(&self) -> Vec<TransactionId> {
        self.edges
            .iter()
            .flat_map(|(waiting_tx, blockers)| {
                std::iter::once(*waiting_tx).chain(blockers.iter().copied())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(super) fn validate(&self) -> DeadlockResult<()> {
        for (waiting_tx, blockers) in &self.edges {
            validate_transaction_id(*waiting_tx)?;
            for blocking_tx in blockers {
                validate_transaction_id(*blocking_tx)?;
                validate_not_self_edge(*waiting_tx, *blocking_tx)?;
            }
        }

        Ok(())
    }
}
