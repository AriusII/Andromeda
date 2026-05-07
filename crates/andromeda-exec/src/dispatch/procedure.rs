use std::sync::Arc;

use andromeda_catalog::{ProcedureContractBinding, ProcedureContractRef};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{InvocationContext, LocalProcedure};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureDispatchRequest {
    pub invocation_id: InvocationId,
    pub procedure: ProcedureContractRef,
    pub procedure_binding: Option<ProcedureContractBinding>,
    pub context: InvocationContext,
    pub pre_transaction: PreTransactionDispatchEvidence,
}

impl ProcedureDispatchRequest {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.invocation_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "Procedure dispatch invocation id must not be zero before handler execution",
            ));
        }
        self.procedure.validate()?;
        let binding = self.procedure_binding.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "Procedure dispatch requires full ProcedureContractBinding before handler execution",
            )
        })?;
        binding.validate()?;
        if binding.as_legacy_ref() != self.procedure {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "Procedure dispatch binding must match dispatch contract before handler execution",
            ));
        }
        self.pre_transaction
            .validate_for_trace(self.context.trace_id)
    }
}

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
    pub fn into_error(self) -> AndromedaError {
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
        Err(self.reason.into_error())
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

#[derive(Clone)]
pub struct SrplDispatcherAdapter {
    dispatcher: crate::SrplProcedureDispatcher,
    local_dispatcher: Option<Arc<dyn ProcedureDispatcher + Send + Sync>>,
}

impl SrplDispatcherAdapter {
    pub fn new(dispatcher: crate::SrplProcedureDispatcher) -> Self {
        Self {
            dispatcher,
            local_dispatcher: None,
        }
    }

    pub fn with_local_dispatcher<D>(
        dispatcher: crate::SrplProcedureDispatcher,
        local_dispatcher: D,
    ) -> Self
    where
        D: ProcedureDispatcher + Send + Sync + 'static,
    {
        Self {
            dispatcher,
            local_dispatcher: Some(Arc::new(local_dispatcher)),
        }
    }

    pub fn with_shared_local_dispatcher(
        dispatcher: crate::SrplProcedureDispatcher,
        local_dispatcher: Arc<dyn ProcedureDispatcher + Send + Sync>,
    ) -> Self {
        Self {
            dispatcher,
            local_dispatcher: Some(local_dispatcher),
        }
    }
}

impl ProcedureDispatcher for SrplDispatcherAdapter {
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<LocalProcedure> {
        request.validate()?;

        let invocation_request = crate::InvocationRequest {
            invocation_id: request.invocation_id,
            procedure: request.procedure,
            expected_binding: request.procedure_binding,
            expected_contract_hash: request.procedure.contract_hash,
            catalog_version: request.procedure.catalog_version,
            structured_parameters: Vec::new(),
        };

        let _procedure = self
            .dispatcher
            .resolve_procedure(&invocation_request)
            .map_err(|resolve_err| {
                AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    format!("SRPL procedure resolution failed: {:?}", resolve_err),
                )
            })?;

        if let Some(local_dispatcher) = &self.local_dispatcher {
            return local_dispatcher.dispatch_procedure(request);
        }

        Err(srpl_local_boundary_contract_error(
            invocation_request.procedure.procedure_id,
        ))
    }
}

fn srpl_local_boundary_contract_error(procedure_id: andromeda_core::ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "SRPL dispatch boundary requires an explicit local handler for resolved ProcedureId {}; no local Procedure dispatcher is configured",
            procedure_id.get()
        ),
    )
}
