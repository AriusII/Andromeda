use andromeda_error::AndromedaResult;
use andromeda_observability::{ExecutionTransitionTrace, TraceId, TransitionReasonCode};
use andromeda_procedure_contract::{
    CompletionEnvelopeVersion, CompletionTerminalCode,
    RpcCompletionStatus as ContractCompletionStatus,
};
use andromeda_transaction::{TransactionState, transaction_phase_code};
use andromeda_types::{InvocationId, RequestId, SessionId, TransactionId};
use andromeda_wal::Lsn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionStatus {
    Committed,
    RolledBack,
    FailedBeforeTransaction,
    Cancelled,
    Poisoned,
    PermissionDenied,
    ContractRejected,
    SystemUnavailable,
}

impl CompletionStatus {
    /// Explicit adapter from the execution-side status into the canonical
    /// procedure completion contract.
    pub const fn to_contract_status(self) -> ContractCompletionStatus {
        match self {
            Self::Committed => ContractCompletionStatus::Committed,
            Self::RolledBack => ContractCompletionStatus::RolledBack,
            Self::FailedBeforeTransaction => ContractCompletionStatus::FailedBeforeTransaction,
            Self::Cancelled => ContractCompletionStatus::Cancelled,
            Self::Poisoned => ContractCompletionStatus::Poisoned,
            Self::PermissionDenied => ContractCompletionStatus::PermissionDenied,
            Self::ContractRejected => ContractCompletionStatus::ContractRejected,
            Self::SystemUnavailable => ContractCompletionStatus::SystemUnavailable,
        }
    }

    /// Explicit adapter from the canonical procedure completion contract into
    /// the execution-side status.
    pub const fn from_contract_status(status: ContractCompletionStatus) -> Self {
        match status {
            ContractCompletionStatus::Committed => Self::Committed,
            ContractCompletionStatus::RolledBack => Self::RolledBack,
            ContractCompletionStatus::FailedBeforeTransaction => Self::FailedBeforeTransaction,
            ContractCompletionStatus::Cancelled => Self::Cancelled,
            ContractCompletionStatus::Poisoned => Self::Poisoned,
            ContractCompletionStatus::PermissionDenied => Self::PermissionDenied,
            ContractCompletionStatus::ContractRejected => Self::ContractRejected,
            ContractCompletionStatus::SystemUnavailable => Self::SystemUnavailable,
        }
    }

    /// Stable terminal completion code owned by
    /// `andromeda-procedure-contract`.
    pub const fn terminal_code(self) -> CompletionTerminalCode {
        self.to_contract_status().terminal_code()
    }

    /// Returns true when the status reports a transaction that reached a
    /// durable terminal state (committed or rolled back).
    pub const fn is_transactional_terminal(self) -> bool {
        self.to_contract_status().is_transactional_terminal()
    }

    /// Maps the completion status onto the stable
    /// [`TransitionReasonCode`] used by execution-side transition traces so
    /// observability sinks can route by reason without re-deriving it.
    pub const fn transition_reason_code(self) -> TransitionReasonCode {
        match self {
            Self::Committed | Self::RolledBack => TransitionReasonCode::DURABLE_WAL_FLUSH,
            Self::FailedBeforeTransaction => TransitionReasonCode::PRE_TRANSACTION_REJECTION,
            Self::Cancelled => TransitionReasonCode::CANCELLED,
            Self::Poisoned => TransitionReasonCode::POISON,
            Self::PermissionDenied => TransitionReasonCode::PERMISSION_DENIED,
            Self::ContractRejected => TransitionReasonCode::PRE_TRANSACTION_REJECTION,
            Self::SystemUnavailable => TransitionReasonCode::SYSTEM_UNAVAILABLE,
        }
    }
}

/// Completion envelope contract version adapted from the canonical procedure
/// completion contract. ResultStream owns executor state, not terminal code
/// integers or completion versioning.
pub const COMPLETION_ENVELOPE_VERSION: CompletionEnvelopeVersion =
    andromeda_procedure_contract::COMPLETION_ENVELOPE_CONTRACT_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationCompletion {
    pub invocation_id: InvocationId,
    pub status: CompletionStatus,
    pub rows_affected: Option<u64>,
    pub transaction_state: Option<TransactionState>,
    pub durable_lsn: Option<Lsn>,
    pub trace_id: TraceId,
}

impl InvocationCompletion {
    pub fn committed(
        invocation_id: InvocationId,
        rows_affected: u64,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<Self> {
        let completion = Self {
            invocation_id,
            status: CompletionStatus::Committed,
            rows_affected: Some(rows_affected),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        };
        completion.validate()?;
        Ok(completion)
    }

    pub fn rolled_back(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<Self> {
        let completion = Self {
            invocation_id,
            status: CompletionStatus::RolledBack,
            rows_affected: Some(0),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        };
        completion.validate()?;
        Ok(completion)
    }

    pub fn pre_transaction(
        invocation_id: InvocationId,
        status: CompletionStatus,
        trace_id: TraceId,
    ) -> AndromedaResult<Self> {
        let completion = Self {
            invocation_id,
            status,
            rows_affected: None,
            transaction_state: None,
            durable_lsn: None,
            trace_id,
        };
        completion.validate()?;
        Ok(completion)
    }

    pub const fn invocation_id(&self) -> InvocationId {
        self.invocation_id
    }

    pub const fn status(&self) -> CompletionStatus {
        self.status
    }

    pub const fn rows_affected(&self) -> Option<u64> {
        self.rows_affected
    }

    pub const fn transaction_state(&self) -> Option<TransactionState> {
        self.transaction_state
    }

    pub const fn durable_lsn(&self) -> Option<Lsn> {
        self.durable_lsn
    }

    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    pub fn validate(self) -> AndromedaResult<()> {
        crate::validation::validate_invocation_completion(self)
    }

    /// Project a terminal `InvocationCompletion` into an
    /// [`ExecutionTransitionTrace`] carrying stable invocation/request/
    /// session/transaction correlation. Use this at the completion-emission
    /// boundary so every terminal completion produces auditable transition
    /// evidence, not just sink-routed `CompletionEmittedTrace` payloads.
    ///
    /// `previous_state` is the transaction phase the invocation was in
    /// before reaching the recorded terminal state. Pass `None` for
    /// pre-transaction terminations (e.g. `FailedBeforeTransaction`,
    /// `PermissionDenied`, `ContractRejected`) where no transaction ever
    /// existed.
    pub fn project_transition(
        &self,
        previous_state: Option<TransactionState>,
        request_id: Option<RequestId>,
        session_id: Option<SessionId>,
        transaction_id: Option<TransactionId>,
        reason: impl Into<String>,
    ) -> ExecutionTransitionTrace {
        let next_phase = self.transaction_state.map(transaction_phase_code);
        let prev_phase = previous_state.map(transaction_phase_code);
        let durable_lsn = self.durable_lsn.and_then(|lsn| {
            let raw = lsn.get();
            if raw == 0 { None } else { Some(raw) }
        });
        // Pre-transaction rejection paths must not carry transaction or
        // durable LSN evidence. We strip them here defensively so a misuse
        // upstream does not fabricate durability claims, and `validate()`
        // re-checks the same invariant.
        let denied = matches!(
            self.status,
            CompletionStatus::PermissionDenied
                | CompletionStatus::ContractRejected
                | CompletionStatus::FailedBeforeTransaction
        );
        let (transaction_id, durable_lsn) = if denied {
            (None, None)
        } else {
            (transaction_id, durable_lsn)
        };
        ExecutionTransitionTrace {
            trace_id: self.trace_id,
            invocation_id: self.invocation_id,
            request_id,
            session_id,
            transaction_id,
            completion_code: Some(self.status.terminal_code()),
            prev_phase,
            next_phase,
            durable_lsn,
            reason_code: self.status.transition_reason_code(),
            reason: reason.into(),
        }
    }
}
