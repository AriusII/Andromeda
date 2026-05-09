use std::time::Duration;

use andromeda_types::TransactionId;

use super::{DeadlockVictim, WaitForGraph};

/// Detector outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlockDetectionStatus {
    NoCycle,
    CycleFound {
        victim: DeadlockVictim,
        cycle_participants: Vec<TransactionId>,
    },
    TimedOut {
        reason: &'static str,
        elapsed: Duration,
        timeout: Duration,
    },
    Deferred {
        reason: &'static str,
    },
}

/// Side-effect-free evidence captured for a coordinator-facing deadlock decision.
///
/// The evidence is derived from an owned lock-table snapshot. It contains no
/// references into the lock manager and carries no commit, rollback, abort, or
/// durability semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockDecisionEvidence {
    /// Number of resource entries in the lock-table snapshot that was observed.
    pub lock_snapshot_resource_count: usize,
    /// Deterministic wait-for edges derived from that snapshot.
    pub wait_for_edges: Vec<(TransactionId, TransactionId)>,
}

impl DeadlockDecisionEvidence {
    pub fn new(lock_snapshot_resource_count: usize, graph: &WaitForGraph) -> Self {
        Self {
            lock_snapshot_resource_count,
            wait_for_edges: graph.edges(),
        }
    }

    pub fn wait_for_edge_count(&self) -> usize {
        self.wait_for_edges.len()
    }
}

/// Boundary-safe decision returned to a transaction coordinator.
///
/// `VictimSuggested` is evidence only. The detector never mutates the lock
/// manager and never invokes transaction abort, rollback, or durability code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlockDecision {
    NoCycle {
        evidence: DeadlockDecisionEvidence,
    },
    VictimSuggested {
        victim: DeadlockVictim,
        cycle_participants: Vec<TransactionId>,
        evidence: DeadlockDecisionEvidence,
    },
    Deferred {
        reason: &'static str,
        evidence: DeadlockDecisionEvidence,
    },
    TimedOut {
        reason: &'static str,
        elapsed: Duration,
        timeout: Duration,
        evidence: DeadlockDecisionEvidence,
    },
}

/// Structured deadlock trace outcome for transaction-local audit projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDecisionTraceOutcome {
    NoCycle,
    VictimSuggested,
    Deferred,
    TimedOut,
}

/// Correlation-friendly deadlock decision trace.
///
/// This projection contains detector facts only. It does not mutate the lock
/// table or transaction manager and intentionally carries no durable LSN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockDecisionTrace {
    /// The suggested victim transaction for `VictimSuggested`; otherwise `None`.
    pub tx_id: Option<TransactionId>,
    pub wait_for_edges: Vec<(TransactionId, TransactionId)>,
    pub cycle_participants: Vec<TransactionId>,
    pub outcome: DeadlockDecisionTraceOutcome,
    pub reason: &'static str,
    pub lock_snapshot_resource_count: usize,
}

impl DeadlockDecision {
    /// Project this decision into local audit evidence.
    ///
    /// The projection reports a victim suggestion, timeout, deferral, or no-cycle
    /// observation only; it is not an abort/rollback instruction.
    pub fn to_trace(&self) -> DeadlockDecisionTrace {
        match self {
            Self::NoCycle { evidence } => DeadlockDecisionTrace {
                tx_id: None,
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: Vec::new(),
                outcome: DeadlockDecisionTraceOutcome::NoCycle,
                reason: "deadlock_detection_found_no_cycle",
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
            Self::VictimSuggested {
                victim,
                cycle_participants,
                evidence,
            } => DeadlockDecisionTrace {
                tx_id: Some(victim.tx_id),
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: cycle_participants.clone(),
                outcome: DeadlockDecisionTraceOutcome::VictimSuggested,
                reason: "deadlock_detection_suggested_victim",
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
            Self::Deferred { reason, evidence } => DeadlockDecisionTrace {
                tx_id: None,
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: Vec::new(),
                outcome: DeadlockDecisionTraceOutcome::Deferred,
                reason,
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
            Self::TimedOut {
                reason, evidence, ..
            } => DeadlockDecisionTrace {
                tx_id: None,
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: Vec::new(),
                outcome: DeadlockDecisionTraceOutcome::TimedOut,
                reason,
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
        }
    }
}

pub(super) fn decision_from_detection_status(
    status: DeadlockDetectionStatus,
    evidence: DeadlockDecisionEvidence,
) -> DeadlockDecision {
    match status {
        DeadlockDetectionStatus::NoCycle => DeadlockDecision::NoCycle { evidence },
        DeadlockDetectionStatus::CycleFound {
            victim,
            cycle_participants,
        } => DeadlockDecision::VictimSuggested {
            victim,
            cycle_participants,
            evidence,
        },
        DeadlockDetectionStatus::TimedOut {
            reason,
            elapsed,
            timeout,
        } => DeadlockDecision::TimedOut {
            reason,
            elapsed,
            timeout,
            evidence,
        },
        DeadlockDetectionStatus::Deferred { reason } => {
            DeadlockDecision::Deferred { reason, evidence }
        },
    }
}
