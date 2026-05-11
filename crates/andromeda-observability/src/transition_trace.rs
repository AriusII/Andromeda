use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{InvocationId, RequestId, SessionId, TransactionId};

use crate::TraceId;

use super::{CriticalDecisionKind, CriticalDecisionTrace};

/// Stable wire-aligned numeric code for a transaction lifecycle phase.
///
/// `andromeda-observe` must remain independent of transaction runtime crates, so phases
/// are projected through this newtype rather than referencing
/// `TransactionState` directly. The numeric encoding is part
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

    // Pre-transaction execution lifecycle phases (codes 10-15).
    // These represent the ordered stages an invocation passes through before
    // any transaction is allocated; they are emitted as `ExecutionTransitionTrace`
    // events by `andromeda-exec` during the dispatch pipeline.
    //
    // Codes are stable wire values — never re-number or delete.

    /// Invocation has been admitted to the local execution runtime and is
    /// pending contract binding.
    pub const ADMITTED: Self = Self(10);

    /// Contract hash and binding have been verified against the catalog
    /// snapshot.
    pub const CONTRACT_BOUND: Self = Self(11);

    /// IAM / permission evaluation passed; invocation is authorized.
    pub const PERMISSION_CHECKED: Self = Self(12);

    /// IO budget has been reserved; the invocation holds a resource grant.
    pub const BUDGET_RESERVED: Self = Self(13);

    /// A transaction id has been allocated and the transaction state-machine
    /// is in the `Active` state.
    pub const TRANSACTION_OPENED: Self = Self(14);

    /// The mutation payload is being dispatched to the executor.
    pub const EXECUTING: Self = Self(15);

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
        matches!(self.0, 1..=15)
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
    pub const TRANSACTION_TIMEOUT: Self = Self(10);
    pub const DEADLOCK_VICTIM: Self = Self(11);

    pub const fn new(code: u16) -> Self {
        Self(code)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Forensic record of a single transaction state-machine transition.
///
/// Emitted by transaction owners when a transaction crosses phase boundaries.
/// Carries enough
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
            return Err(observability_error(
                "transaction transition trace_id must be non-zero",
            ));
        }
        if self.transaction_id.get() == 0 {
            return Err(observability_error(
                "transaction transition transaction_id must be non-zero",
            ));
        }
        if self.invocation_id.is_some_and(|id| id.get() == 0) {
            return Err(observability_error(
                "transaction transition invocation_id must be non-zero when present",
            ));
        }
        if self.request_id.is_some_and(|id| id.get() == 0) {
            return Err(observability_error(
                "transaction transition request_id must be non-zero when present",
            ));
        }
        if self.session_id.is_some_and(|id| id.get() == 0) {
            return Err(observability_error(
                "transaction transition session_id must be non-zero when present",
            ));
        }
        if !self.prev_phase.is_known() || !self.next_phase.is_known() {
            return Err(observability_error(
                "transaction transition phase codes must reference a known phase",
            ));
        }
        if !self.has_reason() {
            return Err(observability_error(
                "transaction transition trace requires a non-empty reason",
            ));
        }
        if !self.proves_terminal_evidence() {
            return Err(observability_error(
                "transaction transition into a terminal commit/rollback phase requires non-zero durable_lsn evidence",
            ));
        }
        if self.durable_lsn.is_some_and(|lsn| lsn == 0) {
            return Err(observability_error(
                "transaction transition durable_lsn must be non-zero when present",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(observability_error(
                "transaction transition reason must not include secrets, key material, tokens, passwords, or payload bodies",
            ));
        }
        Ok(())
    }

    pub fn as_decision_trace(&self) -> CriticalDecisionTrace {
        let decision = if self.next_phase == TransactionPhaseCode::COMMITTED {
            CriticalDecisionKind::CommitVisible
        } else if self.next_phase == TransactionPhaseCode::ROLLED_BACK {
            CriticalDecisionKind::RollbackDurable
        } else {
            CriticalDecisionKind::TransactionCommit
        };
        CriticalDecisionTrace {
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
            return Err(observability_error(
                "execution transition trace_id must be non-zero",
            ));
        }
        if self.invocation_id.get() == 0 {
            return Err(observability_error(
                "execution transition invocation_id must be non-zero",
            ));
        }
        if self.request_id.is_some_and(|id| id.get() == 0) {
            return Err(observability_error(
                "execution transition request_id must be non-zero when present",
            ));
        }
        if self.session_id.is_some_and(|id| id.get() == 0) {
            return Err(observability_error(
                "execution transition session_id must be non-zero when present",
            ));
        }
        if self.transaction_id.is_some_and(|id| id.get() == 0) {
            return Err(observability_error(
                "execution transition transaction_id must be non-zero when present",
            ));
        }
        match self.prev_phase {
            Some(prev) if !prev.is_known() => {
                return Err(observability_error(
                    "execution transition prev_phase must reference a known phase",
                ));
            },
            _ => {},
        }
        match self.next_phase {
            Some(next) if !next.is_known() => {
                return Err(observability_error(
                    "execution transition next_phase must reference a known phase",
                ));
            },
            _ => {},
        }
        if !self.has_reason() {
            return Err(observability_error(
                "execution transition trace requires a non-empty reason",
            ));
        }
        if !self.proves_terminal_evidence() {
            return Err(observability_error(
                "execution transition into a terminal commit/rollback phase requires non-zero durable_lsn evidence",
            ));
        }
        if self.durable_lsn.is_some_and(|lsn| lsn == 0) {
            return Err(observability_error(
                "execution transition durable_lsn must be non-zero when present",
            ));
        }
        let denied = matches!(
            self.reason_code,
            TransitionReasonCode::PERMISSION_DENIED
                | TransitionReasonCode::PRE_TRANSACTION_REJECTION
        );
        if denied && (self.transaction_id.is_some() || self.durable_lsn.is_some()) {
            return Err(observability_error(
                "execution transition with a pre-transaction rejection reason must not carry transaction or durable_lsn evidence",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(observability_error(
                "execution transition reason must not include secrets, key material, tokens, passwords, or payload bodies",
            ));
        }
        Ok(())
    }

    pub fn as_decision_trace(&self) -> CriticalDecisionTrace {
        let decision = match self.next_phase {
            Some(phase) if phase == TransactionPhaseCode::COMMITTED => {
                CriticalDecisionKind::CommitVisible
            },
            Some(phase) if phase == TransactionPhaseCode::ROLLED_BACK => {
                CriticalDecisionKind::RollbackDurable
            },
            _ => match self.reason_code {
                TransitionReasonCode::PERMISSION_DENIED => {
                    CriticalDecisionKind::AuthorizationDenial
                },
                TransitionReasonCode::PRE_TRANSACTION_REJECTION => {
                    CriticalDecisionKind::ContractRejected
                },
                _ => CriticalDecisionKind::CompletionEmitted,
            },
        };
        CriticalDecisionTrace {
            trace_id: self.trace_id,
            decision,
            reason: self.reason.clone(),
        }
    }
}

fn observability_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}

fn contains_sensitive_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "-----begin",
        "private key",
        "private_key",
        "bearer ",
        "credential=",
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "authorization:",
        "x-api-key",
        "payload:",
        "payload body",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_transaction_transition_requires_durable_lsn() {
        let trace = TransactionTransitionTrace {
            trace_id: TraceId::new(70),
            transaction_id: TransactionId::new(901),
            invocation_id: Some(InvocationId::new(11)),
            request_id: Some(RequestId::new(3)),
            session_id: Some(SessionId::new(4)),
            prev_phase: TransactionPhaseCode::COMMITTING,
            next_phase: TransactionPhaseCode::COMMITTED,
            durable_lsn: None,
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "claims terminal commit without durable LSN".to_string(),
        };

        let err = trace.validate().unwrap_err();
        assert!(err.message().contains("durable_lsn evidence"));
    }

    #[test]
    fn pre_transaction_execution_rejection_strips_transaction_evidence() {
        let trace = ExecutionTransitionTrace {
            trace_id: TraceId::new(82),
            invocation_id: InvocationId::new(33),
            request_id: Some(RequestId::new(9)),
            session_id: Some(SessionId::new(10)),
            transaction_id: Some(TransactionId::new(99)),
            completion_code: Some(8),
            prev_phase: None,
            next_phase: None,
            durable_lsn: None,
            reason_code: TransitionReasonCode::PRE_TRANSACTION_REJECTION,
            reason: "contract validation failed before any transaction".to_string(),
        };

        let err = trace.validate().unwrap_err();
        assert!(err.message().contains("must not carry transaction"));
    }

    #[test]
    fn transition_reason_rejects_sensitive_markers() {
        let trace = ExecutionTransitionTrace {
            trace_id: TraceId::new(1),
            invocation_id: InvocationId::new(2),
            request_id: None,
            session_id: None,
            transaction_id: None,
            completion_code: Some(8),
            prev_phase: None,
            next_phase: None,
            durable_lsn: None,
            reason_code: TransitionReasonCode::SYSTEM_UNAVAILABLE,
            reason: "payload: raw body".to_string(),
        };

        let err = trace.validate().unwrap_err();
        assert!(err.message().contains("payload bodies"));
    }

    #[test]
    fn stable_phase_codes_cover_current_contract() {
        assert_eq!(TransactionPhaseCode::CREATED.get(), 1);
        assert_eq!(TransactionPhaseCode::DISPOSED.get(), 9);
        assert_eq!(TransactionPhaseCode::ADMITTED.get(), 10);
        assert_eq!(TransactionPhaseCode::EXECUTING.get(), 15);
        assert!(TransactionPhaseCode::new(9).is_known());
        assert!(TransactionPhaseCode::new(10).is_known());
        assert!(TransactionPhaseCode::new(15).is_known());
        assert!(!TransactionPhaseCode::new(16).is_known());
    }
}
