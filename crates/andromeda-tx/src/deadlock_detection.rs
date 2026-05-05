//! Deadlock detection vocabulary and wait-for graph skeleton.
//!
//! This module is deliberately storage-agnostic and does not wire any abort or
//! rollback policy into [`crate::TransactionManager`]. It provides the stable
//! transaction-level types needed by later detector work.
//!
//! State/event checklist for this skeleton:
//! - States: empty wait-for graph, graph with waiting edges, no cycle reported,
//!   cycle found, detection deferred.
//! - Events: insert wait edge, remove wait edge, remove transaction, run detector.
//! - Legal transitions: empty -> edge-bearing on validated edge insertion;
//!   edge-bearing -> empty or reduced graph on removal; detector may report
//!   no-cycle for empty graphs or defer non-empty analysis until the bounded
//!   algorithm TODO lands.
//! - Illegal transitions: zero transaction identifiers and self-edges are
//!   rejected; timeout policies must be non-zero and bounded.
//! - Audit: all edge storage is deterministic via ordered maps/sets; no abort
//!   decision is emitted by the detector skeleton.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

/// Public deadlock error type.
pub type DeadlockError = AndromedaError;

/// Public deadlock result type.
pub type DeadlockResult<T> = AndromedaResult<T>;

/// Maximum detector timeout accepted by the V0 policy skeleton.
pub const MAX_DEADLOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// Default detector timeout accepted by the V0 policy skeleton.
pub const DEFAULT_DEADLOCK_TIMEOUT: Duration = Duration::from_secs(5);

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

    fn validate(&self) -> DeadlockResult<()> {
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

/// Deterministic policy for selecting a victim after a cycle is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockVictimPolicy {
    /// Choose the greatest transaction identifier.
    YoungestTransactionId,
    /// Choose the smallest transaction identifier.
    OldestTransactionId,
}

/// Bounded deadlock policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockPolicy {
    pub detection_timeout: Duration,
    pub victim_policy: DeadlockVictimPolicy,
}

impl DeadlockPolicy {
    pub fn new(
        detection_timeout: Duration,
        victim_policy: DeadlockVictimPolicy,
    ) -> DeadlockResult<Self> {
        let policy = Self {
            detection_timeout,
            victim_policy,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(self) -> DeadlockResult<()> {
        if self.detection_timeout.is_zero() {
            return Err(deadlock_error("deadlock detection timeout must not be zero"));
        }

        if self.detection_timeout > MAX_DEADLOCK_TIMEOUT {
            return Err(deadlock_error("deadlock detection timeout exceeds V0 bound"));
        }

        Ok(())
    }
}

impl Default for DeadlockPolicy {
    fn default() -> Self {
        Self {
            detection_timeout: DEFAULT_DEADLOCK_TIMEOUT,
            victim_policy: DeadlockVictimPolicy::YoungestTransactionId,
        }
    }
}

/// Deterministic deadlock victim description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockVictim {
    pub tx_id: TransactionId,
    pub victim_policy: DeadlockVictimPolicy,
    pub cycle_participants: Vec<TransactionId>,
}

impl DeadlockVictim {
    pub fn new(
        tx_id: TransactionId,
        victim_policy: DeadlockVictimPolicy,
        cycle_participants: Vec<TransactionId>,
    ) -> DeadlockResult<Self> {
        validate_transaction_id(tx_id)?;
        validate_cycle_participants(&cycle_participants)?;

        if !cycle_participants.contains(&tx_id) {
            return Err(deadlock_error(
                "deadlock victim must be a member of cycle participants",
            ));
        }

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_unique(cycle_participants),
        })
    }

    pub fn from_cycle(
        cycle_participants: Vec<TransactionId>,
        victim_policy: DeadlockVictimPolicy,
    ) -> DeadlockResult<Self> {
        validate_cycle_participants(&cycle_participants)?;

        let sorted_participants = sorted_unique(cycle_participants);
        let tx_id = match victim_policy {
            DeadlockVictimPolicy::YoungestTransactionId => sorted_participants
                .last()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
            DeadlockVictimPolicy::OldestTransactionId => sorted_participants
                .first()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
        };

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_participants,
        })
    }
}

/// Detector outcome used by the skeleton and later algorithm TODOs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlockDetectionStatus {
    NoCycle,
    CycleFound {
        victim: DeadlockVictim,
        cycle_participants: Vec<TransactionId>,
    },
    Deferred {
        reason: &'static str,
    },
}

/// Deadlock detector skeleton.
///
/// This type validates policy and graph inputs. It intentionally does not run a
/// full DFS/SCC cycle detection algorithm yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockDetector {
    policy: DeadlockPolicy,
}

impl DeadlockDetector {
    pub fn new(policy: DeadlockPolicy) -> DeadlockResult<Self> {
        policy.validate()?;
        Ok(Self { policy })
    }

    pub fn policy(self) -> DeadlockPolicy {
        self.policy
    }

    pub fn detect(&self, graph: &WaitForGraph) -> DeadlockResult<DeadlockDetectionStatus> {
        self.policy.validate()?;
        graph.validate()?;

        if graph.is_empty() {
            return Ok(DeadlockDetectionStatus::NoCycle);
        }

        Ok(DeadlockDetectionStatus::Deferred {
            reason: "deadlock cycle detection is deferred to the detector algorithm TODO",
        })
    }
}

impl Default for DeadlockDetector {
    fn default() -> Self {
        Self {
            policy: DeadlockPolicy::default(),
        }
    }
}

fn validate_transaction_id(tx_id: TransactionId) -> DeadlockResult<()> {
    if tx_id.get() == 0 {
        return Err(deadlock_error("deadlock transaction id must not be zero"));
    }

    Ok(())
}

fn validate_not_self_edge(
    waiting_tx: TransactionId,
    blocking_tx: TransactionId,
) -> DeadlockResult<()> {
    if waiting_tx == blocking_tx {
        return Err(deadlock_error("deadlock wait-for self edge is invalid"));
    }

    Ok(())
}

fn validate_cycle_participants(cycle_participants: &[TransactionId]) -> DeadlockResult<()> {
    if cycle_participants.len() < 2 {
        return Err(deadlock_error(
            "deadlock cycle must contain at least two transactions",
        ));
    }

    for tx_id in cycle_participants {
        validate_transaction_id(*tx_id)?;
    }

    Ok(())
}

fn sorted_unique(mut tx_ids: Vec<TransactionId>) -> Vec<TransactionId> {
    tx_ids.sort();
    tx_ids.dedup();
    tx_ids
}

fn deadlock_error(message: &'static str) -> DeadlockError {
    AndromedaError::new(AndromedaErrorKind::Transaction, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_edge_insertion_is_deterministic_and_idempotent() {
        let mut graph = WaitForGraph::new();

        assert!(
            graph
                .insert_edge(TransactionId::new(3), TransactionId::new(2))
                .unwrap()
        );
        assert!(
            graph
                .insert_edge(TransactionId::new(1), TransactionId::new(4))
                .unwrap()
        );
        assert!(
            graph
                .insert_edge(TransactionId::new(1), TransactionId::new(2))
                .unwrap()
        );
        assert!(
            !graph
                .insert_edge(TransactionId::new(1), TransactionId::new(2))
                .unwrap()
        );

        assert_eq!(
            graph.edges(),
            vec![
                (TransactionId::new(1), TransactionId::new(2)),
                (TransactionId::new(1), TransactionId::new(4)),
                (TransactionId::new(3), TransactionId::new(2)),
            ]
        );
        assert_eq!(graph.edge_count(), 3);
    }

    #[test]
    fn graph_edge_removal_removes_empty_waiter_entries() {
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(3))
            .unwrap();

        assert!(
            graph
                .remove_edge(TransactionId::new(1), TransactionId::new(2))
                .unwrap()
        );
        assert_eq!(
            graph.blockers_for(TransactionId::new(1)).unwrap(),
            vec![TransactionId::new(3)]
        );
        assert!(
            graph
                .remove_edge(TransactionId::new(1), TransactionId::new(3))
                .unwrap()
        );
        assert!(graph.is_empty());
        assert!(
            !graph
                .remove_edge(TransactionId::new(1), TransactionId::new(3))
                .unwrap()
        );
    }

    #[test]
    fn graph_remove_transaction_removes_incoming_and_outgoing_edges() {
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(3))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(4), TransactionId::new(2))
            .unwrap();

        assert!(graph.remove_transaction(TransactionId::new(2)).unwrap());
        assert_eq!(graph.edges(), Vec::<(TransactionId, TransactionId)>::new());
    }

    #[test]
    fn zero_transaction_ids_are_rejected() {
        let mut graph = WaitForGraph::new();

        assert!(
            graph
                .insert_edge(TransactionId::new(0), TransactionId::new(1))
                .is_err()
        );
        assert!(
            graph
                .insert_edge(TransactionId::new(1), TransactionId::new(0))
                .is_err()
        );
        assert!(graph.remove_transaction(TransactionId::new(0)).is_err());
        assert!(
            DeadlockVictim::from_cycle(
                vec![TransactionId::new(1), TransactionId::new(0)],
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .is_err()
        );
    }

    #[test]
    fn victim_construction_is_deterministic() {
        let victim = DeadlockVictim::from_cycle(
            vec![
                TransactionId::new(5),
                TransactionId::new(2),
                TransactionId::new(9),
                TransactionId::new(5),
            ],
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap();

        assert_eq!(victim.tx_id, TransactionId::new(9));
        assert_eq!(
            victim.cycle_participants,
            vec![
                TransactionId::new(2),
                TransactionId::new(5),
                TransactionId::new(9),
            ]
        );

        let oldest = DeadlockVictim::from_cycle(
            vec![TransactionId::new(5), TransactionId::new(2)],
            DeadlockVictimPolicy::OldestTransactionId,
        )
        .unwrap();
        assert_eq!(oldest.tx_id, TransactionId::new(2));
    }

    #[test]
    fn default_policy_is_valid_and_timeout_is_bounded() {
        assert!(DeadlockPolicy::default().validate().is_ok());
        assert!(
            DeadlockPolicy::new(Duration::ZERO, DeadlockVictimPolicy::YoungestTransactionId)
                .is_err()
        );
        assert!(
            DeadlockPolicy::new(
                MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .is_err()
        );
    }

    #[test]
    fn skeleton_detector_reports_no_cycle_for_empty_graph() {
        let detector = DeadlockDetector::new(DeadlockPolicy::default()).unwrap();
        let graph = WaitForGraph::new();

        assert_eq!(
            detector.detect(&graph).unwrap(),
            DeadlockDetectionStatus::NoCycle
        );
    }

    #[test]
    fn skeleton_detector_defers_non_empty_graph_detection() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();

        assert!(matches!(
            detector.detect(&graph).unwrap(),
            DeadlockDetectionStatus::Deferred { .. }
        ));
    }
}
