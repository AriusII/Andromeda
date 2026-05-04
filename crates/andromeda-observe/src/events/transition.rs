use andromeda_core::{AndromedaResult, InvocationId, RequestId, SessionId, TransactionId};

use crate::TraceId;

use super::{contains_sensitive_marker, observe_error, CriticalDecisionKind, DecisionTrace};

/// Stable wire-aligned numeric code for a transaction lifecycle phase.
///
/// `andromeda-observe` must remain independent of `andromeda-tx`, so phases
/// are projected through this newtype rather than referencing
/// `andromeda_tx::TransactionState` directly. The numeric encoding is part
/// of the durable observability contract: codes 1..=9 are stable and any
/// change is a breaking change to the transition trace contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionPhaseCode(u16);

impl TransactionPhaseCode {
    pub const CREATED: Self = Self(1);
    pub const ACTIVE: Self = Self(2);
    pub const COMMITTING: Self = Self(3);
    pub const COMMITTED: Self = Self(4);
    pub const FAILED: Self = Self(5);
    pub const ROLLING_BACK: Self = Self(6);
    pub const ROLLED_BACK: Self = Self(7);
    pub const POISONED: Self = Self(8);
    pub const DISPOSED: Self = Self(9);

    pub const fn new(code: u16) -> Self {
        Self(code)
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::COMMITTED | Self::ROLLED_BACK | Self::DISPOSED)
    }

    pub const fn requires_durable_evidence(self) -> bool {
        matches!(self, Self::COMMITTED | Self::ROLLED_BACK)
    }

    pub const fn is_known(self) -> bool {
        matches!(self.0, 1..=9)
    }
}

/// Stable numeric code for the reason driving a transaction or execution
/// transition. Codes are stable across releases; new reasons must be appended
/// rather than re-numbering existing ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionReasonCode(u16);

impl TransitionReasonCode {
    pub const UNSPECIFIED: Self = Self(0);
    pub const NORMAL_PROGRESS: Self = Self(1);
    pub const DURABLE_WAL_FLUSH: Self = Self(2);
    pub const CALLER_ROLLBACK: Self = Self(3);
    pub const EXECUTOR_FAILURE: Self = Self(4);
    pub const POISON: Self = Self(5);
    pub const PRE_TRANSACTION_REJECTION: Self = Self(6);
    pub const PERMISSION_DENIED: Self = Self(7);
    pub const SYSTEM_UNAVAILABLE: Self = Self(8);
    pub const CANCELLED: Self = Self(9);

    pub const fn new(code: u16) -> Self {
        Self(code)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Forensic record of a single transaction state-machine transition.
///
/// Emitted by `andromeda-tx` (and any caller projecting a `TransactionStateMachine`
/// snapshot) when a transaction crosses phase boundaries. Carries enough
/// correlation to be cross-referenced with the request that initiated the
/// transaction and the durable WAL evidence that backs any terminal claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionTransitionTrace {
    pub trace_id: TraceId,
    pub transaction_id: TransactionId,
    pub invocation_id: Option<InvocationId>,
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub prev_phase: TransactionPhaseCode,
    pub next_phase: TransactionPhaseCode,
    pub durable_lsn: Option<u64>,
    pub reason_code: TransitionReasonCode,
    pub reason: String,
}

impl TransactionTransitionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn proves_terminal_evidence(&self) -> bool {
        if !self.next_phase.requires_durable_evidence() {
            return true;
        }
        match self.durable_lsn {
            Some(lsn) => lsn != 0,
            None => false,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_zero() {
            return Err(observe_error(
                "transaction transition trace_id must be non-zero",
            ));
        }
        if self.transaction_id.get() == 0 {
            return Err(observe_error(
                "transaction transition transaction_id must be non-zero",
            ));
        }
        if self.invocation_id.is_some_and(|id| id.get() == 0) {
            return Err(observe_error(
                "transaction transition invocation_id must be non-zero when present",
            ));
        }
        if self.request_id.is_some_and(|id| id.get() == 0) {
            return Err(observe_error(
                "transaction transition request_id must be non-zero when present",
            ));
        }
        if self.session_id.is_some_and(|id| id.get() == 0) {
            return Err(observe_error(
                "transaction transition session_id must be non-zero when present",
            ));
        }
        if !self.prev_phase.is_known() || !self.next_phase.is_known() {
            return Err(observe_error(
                "transaction transition phase codes must reference a known phase",
            ));
        }
        if !self.has_reason() {
            return Err(observe_error(
                "transaction transition trace requires a non-empty reason",
            ));
        }
        if !self.proves_terminal_evidence() {
            return Err(observe_error(
                "transaction transition into a terminal commit/rollback phase requires non-zero durable_lsn evidence",
            ));
        }
        if self.durable_lsn.is_some_and(|lsn| lsn == 0) {
            return Err(observe_error(
                "transaction transition durable_lsn must be non-zero when present",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(observe_error(
                "transaction transition reason must not include secrets, key material, tokens, passwords, or payload bodies",
            ));
        }
        Ok(())
    }

    pub fn as_decision_trace(&self) -> DecisionTrace {
        let decision = if self.next_phase == TransactionPhaseCode::COMMITTED {
            CriticalDecisionKind::CommitVisible
        } else if self.next_phase == TransactionPhaseCode::ROLLED_BACK {
            CriticalDecisionKind::RollbackDurable
        } else {
            CriticalDecisionKind::TransactionCommit
        };
        DecisionTrace {
            trace_id: self.trace_id,
            decision,
            reason: self.reason.clone(),
        }
    }
}

/// Forensic record of a single execution-side invocation transition.
///
/// Emitted by `andromeda-exec` when an invocation crosses a lifecycle
/// boundary: pre-transaction admission/rejection, transaction binding, and
/// terminal completion. Carries the wire-aligned `CompletionStatus` terminal
/// code so executor telemetry, journals, and recovery comparisons never
/// depend on `Debug`/`Display` projections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTransitionTrace {
    pub trace_id: TraceId,
    pub invocation_id: InvocationId,
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub transaction_id: Option<TransactionId>,
    pub completion_code: Option<u32>,
    pub prev_phase: Option<TransactionPhaseCode>,
    pub next_phase: Option<TransactionPhaseCode>,
    pub durable_lsn: Option<u64>,
    pub reason_code: TransitionReasonCode,
    pub reason: String,
}

impl ExecutionTransitionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn proves_terminal_evidence(&self) -> bool {
        match self.next_phase {
            Some(phase) if phase.requires_durable_evidence() => match self.durable_lsn {
                Some(lsn) => lsn != 0,
                None => false,
            },
            _ => true,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_zero() {
            return Err(observe_error(
                "execution transition trace_id must be non-zero",
            ));
        }
        if self.invocation_id.get() == 0 {
            return Err(observe_error(
                "execution transition invocation_id must be non-zero",
            ));
        }
        if self.request_id.is_some_and(|id| id.get() == 0) {
            return Err(observe_error(
                "execution transition request_id must be non-zero when present",
            ));
        }
        if self.session_id.is_some_and(|id| id.get() == 0) {
            return Err(observe_error(
                "execution transition session_id must be non-zero when present",
            ));
        }
        if self.transaction_id.is_some_and(|id| id.get() == 0) {
            return Err(observe_error(
                "execution transition transaction_id must be non-zero when present",
            ));
        }
        if let Some(prev) = self.prev_phase {
            if !prev.is_known() {
                return Err(observe_error(
                    "execution transition prev_phase must reference a known phase",
                ));
            }
        }
        if let Some(next) = self.next_phase {
            if !next.is_known() {
                return Err(observe_error(
                    "execution transition next_phase must reference a known phase",
                ));
            }
        }
        if !self.has_reason() {
            return Err(observe_error(
                "execution transition trace requires a non-empty reason",
            ));
        }
        if !self.proves_terminal_evidence() {
            return Err(observe_error(
                "execution transition into a terminal commit/rollback phase requires non-zero durable_lsn evidence",
            ));
        }
        if self.durable_lsn.is_some_and(|lsn| lsn == 0) {
            return Err(observe_error(
                "execution transition durable_lsn must be non-zero when present",
            ));
        }
        let denied = matches!(
            self.reason_code,
            TransitionReasonCode::PERMISSION_DENIED
                | TransitionReasonCode::PRE_TRANSACTION_REJECTION
        );
        if denied && (self.transaction_id.is_some() || self.durable_lsn.is_some()) {
            return Err(observe_error(
                "execution transition with a pre-transaction rejection reason must not carry transaction or durable_lsn evidence",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(observe_error(
                "execution transition reason must not include secrets, key material, tokens, passwords, or payload bodies",
            ));
        }
        Ok(())
    }

    pub fn as_decision_trace(&self) -> DecisionTrace {
        let decision = match self.next_phase {
            Some(phase) if phase == TransactionPhaseCode::COMMITTED => {
                CriticalDecisionKind::CommitVisible
            }
            Some(phase) if phase == TransactionPhaseCode::ROLLED_BACK => {
                CriticalDecisionKind::RollbackDurable
            }
            _ => match self.reason_code {
                TransitionReasonCode::PERMISSION_DENIED => {
                    CriticalDecisionKind::AuthorizationDenial
                }
                TransitionReasonCode::PRE_TRANSACTION_REJECTION => {
                    CriticalDecisionKind::ContractRejected
                }
                _ => CriticalDecisionKind::CompletionEmitted,
            },
        };
        DecisionTrace {
            trace_id: self.trace_id,
            decision,
            reason: self.reason.clone(),
        }
    }
}
