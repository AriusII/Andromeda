use andromeda_types::TransactionId;

use super::entry::{LockHolder, LockWaiter};
use super::mode::LockMode;
use super::resource::LockResource;

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

/// Structured lock trace kind for transaction-local audit projection.
///
/// These values are observability evidence only. They do not emit globally and
/// do not imply abort, rollback, commit, WAL, or durability state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockTraceKind {
    CriticalWait,
    Promotion,
    ReleaseAll,
}

/// Structured lock decision outcome for transaction-local audit projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockTraceOutcome {
    Waiting,
    WaitingUpgrade,
    Promoted,
    Released,
    Noop,
}

/// Correlation-friendly lock decision evidence.
///
/// The evidence is deliberately local to `andromeda-tx`: callers may later map
/// it into `andromeda-observe`, but no global emitter is required here. It
/// carries lock-table facts only and intentionally has no durable LSN field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockDecisionEvidence {
    pub kind: LockTraceKind,
    pub tx_id: TransactionId,
    pub resource: Option<LockResource>,
    pub mode: Option<LockMode>,
    pub blockers: Vec<LockHolder>,
    pub waiters: Vec<LockWaiter>,
    pub promoted_waiters: Vec<LockWaiter>,
    pub outcome: LockTraceOutcome,
    pub reason: &'static str,
    pub affected_resource_count: usize,
    pub released_holder_count: usize,
    pub removed_waiter_count: usize,
}

impl LockDecisionEvidence {
    fn critical_wait(
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
        blockers: Vec<LockHolder>,
        outcome: LockTraceOutcome,
        reason: &'static str,
    ) -> Self {
        Self {
            kind: LockTraceKind::CriticalWait,
            tx_id,
            resource: Some(resource),
            mode: Some(mode),
            blockers,
            waiters: Vec::new(),
            promoted_waiters: Vec::new(),
            outcome,
            reason,
            affected_resource_count: 0,
            released_holder_count: 0,
            removed_waiter_count: 0,
        }
    }

    pub(super) fn promotion(resource: LockResource, waiter: LockWaiter) -> Self {
        Self {
            kind: LockTraceKind::Promotion,
            tx_id: waiter.tx_id,
            resource: Some(resource),
            mode: Some(waiter.mode),
            blockers: Vec::new(),
            waiters: Vec::new(),
            promoted_waiters: vec![waiter],
            outcome: LockTraceOutcome::Promoted,
            reason: "fifo_waiter_promoted_after_lock_release",
            affected_resource_count: 0,
            released_holder_count: 0,
            removed_waiter_count: 0,
        }
    }
}

/// Non-breaking acquire hook result that carries the existing status plus any
/// trace evidence for critical waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockAcquireEvidence {
    pub status: LockAcquireStatus,
    pub evidence: Option<LockDecisionEvidence>,
}

/// Non-breaking release hook result that carries the existing boolean outcome
/// plus promotion traces caused by the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockReleaseEvidence {
    pub released_any: bool,
    pub evidence: Vec<LockDecisionEvidence>,
}

/// Non-breaking release-all hook result that carries the existing summary plus
/// release-all and promotion traces caused by cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockReleaseAllEvidence {
    pub summary: LockReleaseAllSummary,
    pub evidence: Vec<LockDecisionEvidence>,
}

impl LockAcquireStatus {
    /// Project a critical-wait trace when this acquire status represents a wait.
    ///
    /// Non-waiting acquire outcomes return `None` to keep the hook focused on
    /// the critical wait invariant.
    pub fn critical_wait_evidence(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> Option<LockDecisionEvidence> {
        match self {
            Self::Waiting { blockers, .. } => Some(LockDecisionEvidence::critical_wait(
                tx_id,
                resource,
                mode,
                blockers.clone(),
                LockTraceOutcome::Waiting,
                "lock_request_waiting_on_incompatible_holders_or_fifo_fairness",
            )),
            Self::WaitingUpgrade {
                requested_mode,
                blockers,
                ..
            } => Some(LockDecisionEvidence::critical_wait(
                tx_id,
                resource,
                *requested_mode,
                blockers.clone(),
                LockTraceOutcome::WaitingUpgrade,
                "lock_upgrade_waiting_on_incompatible_holders_or_fifo_fairness",
            )),
            Self::Granted | Self::AlreadyHeld { .. } | Self::Upgraded { .. } => None,
        }
    }
}

/// Deterministic evidence returned by
/// [`LockManager::release_all`](crate::LockManager::release_all).
///
/// Counts are intentionally order-independent because the backing lock table is
/// a hash map. `release_all` is a transaction-end cleanup primitive only; it is
/// not evidence that a commit or rollback has become durable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LockReleaseAllSummary {
    /// Number of resource entries where at least one holder or waiter belonging
    /// to the transaction was removed.
    pub affected_resource_count: usize,
    /// Number of held lock records removed for the transaction.
    pub released_holder_count: usize,
    /// Number of waiter records removed for the transaction.
    pub removed_waiter_count: usize,
}

impl LockReleaseAllSummary {
    pub const fn removed_any(self) -> bool {
        self.released_holder_count > 0 || self.removed_waiter_count > 0
    }

    /// Project this cleanup summary as trace evidence.
    ///
    /// The projection is cleanup evidence only and intentionally omits any
    /// durable LSN or WAL/visibility outcome.
    pub fn to_trace(self, tx_id: TransactionId) -> LockDecisionEvidence {
        LockDecisionEvidence {
            kind: LockTraceKind::ReleaseAll,
            tx_id,
            resource: None,
            mode: None,
            blockers: Vec::new(),
            waiters: Vec::new(),
            promoted_waiters: Vec::new(),
            outcome: if self.removed_any() {
                LockTraceOutcome::Released
            } else {
                LockTraceOutcome::Noop
            },
            reason: "terminal_lock_cleanup",
            affected_resource_count: self.affected_resource_count,
            released_holder_count: self.released_holder_count,
            removed_waiter_count: self.removed_waiter_count,
        }
    }
}
