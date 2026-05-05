use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{InvocationContext, LocalProcedure};

/// Pre-transaction evidence required before any local or remote Procedure
/// dispatch. This type intentionally carries no transaction or WAL evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreTransactionDispatchEvidence {
    pub admission_trace: DecisionTrace,
    pub contract_trace: DecisionTrace,
    pub authorization_trace: Option<DecisionTrace>,
}

impl PreTransactionDispatchEvidence {
    pub fn validate_for_trace(&self, trace_id: TraceId) -> AndromedaResult<()> {
        validate_decision(
            &self.admission_trace,
            trace_id,
            CriticalDecisionKind::ResourceGovernance,
            "admission",
        )?;
        validate_decision(
            &self.contract_trace,
            trace_id,
            CriticalDecisionKind::ContractValidation,
            "contract validation",
        )?;

        if let Some(trace) = &self.authorization_trace {
            validate_decision(
                trace,
                trace_id,
                CriticalDecisionKind::SecurityAuthorization,
                "authorization",
            )?;
        }

        Ok(())
    }
}

/// Storage- and network-neutral Procedure dispatch request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureDispatchRequest {
    pub procedure: ProcedureContractRef,
    pub context: InvocationContext,
    pub pre_transaction: PreTransactionDispatchEvidence,
}

impl ProcedureDispatchRequest {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.procedure.validate()?;
        self.pre_transaction
            .validate_for_trace(self.context.trace_id)
    }
}

/// Execution boundary for local registry-backed dispatch and future remote
/// dispatch. Implementors must validate pre-transaction evidence before they
/// allocate transactions, append WAL, or perform transport I/O.
pub trait ProcedureDispatcher {
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<LocalProcedure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureDispatchUnavailableReason {
    RemoteDispatchUnsupported,
    RemoteTransportUnavailable,
}

impl ProcedureDispatchUnavailableReason {
    pub fn as_error(self) -> AndromedaError {
        match self {
            Self::RemoteDispatchUnsupported => AndromedaError::new(
                AndromedaErrorKind::Transport,
                "remote Procedure dispatch is unsupported by this dispatcher",
            ),
            Self::RemoteTransportUnavailable => AndromedaError::new(
                AndromedaErrorKind::Transport,
                "remote Procedure dispatch transport is unavailable",
            ),
        }
    }
}

/// Explicit no-I/O placeholder for the future H3 remote dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteProcedureDispatcherUnavailable {
    reason: ProcedureDispatchUnavailableReason,
}

impl RemoteProcedureDispatcherUnavailable {
    pub const fn unsupported() -> Self {
        Self {
            reason: ProcedureDispatchUnavailableReason::RemoteDispatchUnsupported,
        }
    }

    pub const fn unavailable() -> Self {
        Self {
            reason: ProcedureDispatchUnavailableReason::RemoteTransportUnavailable,
        }
    }

    pub const fn reason(self) -> ProcedureDispatchUnavailableReason {
        self.reason
    }
}

impl ProcedureDispatcher for RemoteProcedureDispatcherUnavailable {
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<LocalProcedure> {
        request.validate()?;
        Err(self.reason.as_error())
    }
}

fn validate_decision(
    trace: &DecisionTrace,
    trace_id: TraceId,
    expected: CriticalDecisionKind,
    label: &'static str,
) -> AndromedaResult<()> {
    if trace.trace_id != trace_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} evidence trace id must match invocation context"),
        ));
    }

    if trace.decision != expected {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("pre-transaction {label} evidence required before Procedure dispatch"),
        ));
    }

    if !trace.has_explanation() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("pre-transaction {label} evidence must include an explanation"),
        ));
    }

    Ok(())
}
