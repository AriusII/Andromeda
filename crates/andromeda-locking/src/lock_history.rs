//! Lock history trace structures for observability.
//!
//! This module defines structured history evidence for lock management decisions:
//! - **LockWaitTrace**: Emitted when a transaction waits for a lock
//! - **LockPromotionTrace**: Emitted when a waiter is promoted to holder
//! - **DeadlockDecisionTrace**: Emitted when deadlock detection runs
//!
//! These traces integrate with the `andromeda-observe` history infrastructure and
//! carry correlation evidence for forensic analysis.

use andromeda_core::{EngineTimestamp, TransactionId};

use crate::{LockHolder, LockMode, LockResource};

/// Trace evidence for a transaction waiting for a lock.
///
/// Emitted when a transaction requests a lock that cannot be granted immediately
/// and is enqueued behind incompatible holders or waiter fairness constraints.
#[derive(Clone, Debug)]
pub struct LockWaitTrace {
    /// Transaction ID that initiated the wait
    pub tx_id: TransactionId,
    /// Resource on which the wait occurred
    pub resource_id: LockResource,
    /// Requested lock mode
    pub requested_mode: LockMode,
    /// Transaction IDs currently holding incompatible locks
    pub blocker_tx_ids: Vec<TransactionId>,
    /// Timestamp when the wait was recorded
    pub timestamp: EngineTimestamp,
}

impl LockWaitTrace {
    /// Construct a wait trace from minimal components.
    pub fn new(
        tx_id: TransactionId,
        resource_id: LockResource,
        requested_mode: LockMode,
        blockers: Vec<LockHolder>,
        timestamp: EngineTimestamp,
    ) -> Self {
        Self {
            tx_id,
            resource_id,
            requested_mode,
            blocker_tx_ids: blockers.into_iter().map(|h| h.tx_id).collect(),
            timestamp,
        }
    }
}

/// Trace evidence for a transaction being promoted from waiter to holder.
///
/// Emitted when a holder releases a lock and the next waiter in FIFO order
/// becomes the new holder (when lock modes are compatible).
#[derive(Clone, Debug)]
pub struct LockPromotionTrace {
    /// Transaction ID that was promoted from waiter to holder
    pub tx_id: TransactionId,
    /// Resource on which the promotion occurred
    pub resource_id: LockResource,
    /// Lock mode acquired after promotion
    pub mode: LockMode,
    /// Transaction ID that released the lock (allowing promotion)
    pub released_by_tx: TransactionId,
    /// Timestamp when the promotion was recorded
    pub timestamp: EngineTimestamp,
}

impl LockPromotionTrace {
    /// Construct a promotion trace.
    pub fn new(
        tx_id: TransactionId,
        resource_id: LockResource,
        mode: LockMode,
        released_by_tx: TransactionId,
        timestamp: EngineTimestamp,
    ) -> Self {
        Self {
            tx_id,
            resource_id,
            mode,
            released_by_tx,
            timestamp,
        }
    }
}

/// Deadlock detection decision outcome enumeration.
///
/// Describes the result of deadlock detection in deterministic terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadlockDecisionKind {
    /// No cycle was found in the wait-for graph
    NoCycleDetected,
    /// A cycle was found; a victim transaction has been selected
    VictimSuggested,
    /// Detection was deferred (reserved for future integration)
    Deferred,
    /// Detection deadline was exceeded before completion
    Timeout,
}

/// Trace evidence for a deadlock detection decision.
///
/// Emitted when deadlock detection runs, documenting the cycle detected
/// (if any) and the victim selection decision.
#[derive(Clone, Debug)]
pub struct DeadlockAuditTrace {
    /// Timestamp when detection was initiated
    pub detection_ts: EngineTimestamp,
    /// Transaction IDs in the detected cycle (empty if no cycle)
    pub cycle_txs: Vec<TransactionId>,
    /// Kind of decision reached (no cycle, victim suggested, etc.)
    pub decision: DeadlockDecisionKind,
    /// Victim transaction ID (only Some if decision is VictimSuggested)
    pub victim_tx_id: Option<TransactionId>,
    /// Human-readable reason for the decision
    pub reason: String,
}

impl DeadlockAuditTrace {
    /// Construct a no-cycle decision trace.
    pub fn no_cycle_detected(detection_ts: EngineTimestamp) -> Self {
        Self {
            detection_ts,
            cycle_txs: Vec::new(),
            decision: DeadlockDecisionKind::NoCycleDetected,
            victim_tx_id: None,
            reason: "no cycle detected in wait-for graph".to_string(),
        }
    }

    /// Construct a victim-suggested decision trace.
    pub fn victim_suggested(
        detection_ts: EngineTimestamp,
        cycle_txs: Vec<TransactionId>,
        victim_tx_id: TransactionId,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            detection_ts,
            cycle_txs,
            decision: DeadlockDecisionKind::VictimSuggested,
            victim_tx_id: Some(victim_tx_id),
            reason: reason.into(),
        }
    }

    /// Construct a timeout decision trace.
    pub fn timeout(detection_ts: EngineTimestamp, reason: impl Into<String>) -> Self {
        Self {
            detection_ts,
            cycle_txs: Vec::new(),
            decision: DeadlockDecisionKind::Timeout,
            victim_tx_id: None,
            reason: reason.into(),
        }
    }

    /// Construct a deferred decision trace.
    pub fn deferred(detection_ts: EngineTimestamp, reason: impl Into<String>) -> Self {
        Self {
            detection_ts,
            cycle_txs: Vec::new(),
            decision: DeadlockDecisionKind::Deferred,
            victim_tx_id: None,
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::TransactionId;

    #[test]
    fn lock_wait_trace_captures_wait_evidence() {
        let tx_id = TransactionId::new(1);
        let resource = LockResource::table(1, 10).unwrap();
        let blocker = LockHolder::new(TransactionId::new(2), LockMode::Exclusive).unwrap();
        let ts = EngineTimestamp::from_unix_millis(1000);

        let trace = LockWaitTrace::new(tx_id, resource, LockMode::Shared, vec![blocker], ts);

        assert_eq!(trace.tx_id, tx_id);
        assert_eq!(trace.resource_id, resource);
        assert_eq!(trace.requested_mode, LockMode::Shared);
        assert_eq!(trace.blocker_tx_ids, vec![TransactionId::new(2)]);
        assert_eq!(trace.timestamp, ts);
    }

    #[test]
    fn lock_promotion_trace_captures_promotion_evidence() {
        let tx_id = TransactionId::new(1);
        let released_by_tx = TransactionId::new(2);
        let resource = LockResource::table(1, 10).unwrap();
        let ts = EngineTimestamp::from_unix_millis(1000);

        let trace =
            LockPromotionTrace::new(tx_id, resource, LockMode::Exclusive, released_by_tx, ts);

        assert_eq!(trace.tx_id, tx_id);
        assert_eq!(trace.resource_id, resource);
        assert_eq!(trace.mode, LockMode::Exclusive);
        assert_eq!(trace.released_by_tx, released_by_tx);
        assert_eq!(trace.timestamp, ts);
    }

    #[test]
    fn deadlock_no_cycle_trace_has_empty_cycle_and_no_victim() {
        let ts = EngineTimestamp::from_unix_millis(1000);
        let trace = DeadlockAuditTrace::no_cycle_detected(ts);

        assert_eq!(trace.detection_ts, ts);
        assert_eq!(trace.cycle_txs, Vec::<TransactionId>::new());
        assert_eq!(trace.decision, DeadlockDecisionKind::NoCycleDetected);
        assert_eq!(trace.victim_tx_id, None);
    }

    #[test]
    fn deadlock_victim_suggested_trace_captures_cycle_and_victim() {
        let ts = EngineTimestamp::from_unix_millis(1000);
        let cycle_txs = vec![TransactionId::new(1), TransactionId::new(2)];
        let victim_tx = TransactionId::new(1);
        let trace = DeadlockAuditTrace::victim_suggested(
            ts,
            cycle_txs.clone(),
            victim_tx,
            "youngest transaction by start order",
        );

        assert_eq!(trace.detection_ts, ts);
        assert_eq!(trace.cycle_txs, cycle_txs);
        assert_eq!(trace.decision, DeadlockDecisionKind::VictimSuggested);
        assert_eq!(trace.victim_tx_id, Some(victim_tx));
    }

    #[test]
    fn deadlock_timeout_trace_has_no_cycle_or_victim() {
        let ts = EngineTimestamp::from_unix_millis(1000);
        let trace = DeadlockAuditTrace::timeout(ts, "detection deadline expired");

        assert_eq!(trace.detection_ts, ts);
        assert_eq!(trace.cycle_txs, Vec::<TransactionId>::new());
        assert_eq!(trace.decision, DeadlockDecisionKind::Timeout);
        assert_eq!(trace.victim_tx_id, None);
    }
}
