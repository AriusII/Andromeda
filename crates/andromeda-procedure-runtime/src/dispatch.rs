use std::{marker::PhantomData, sync::Arc};

use andromeda_admission::{InvocationContext, InvocationRequest};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_procedure_contract::{ProcedureContractBinding, ProcedureContractRef};
use andromeda_types::{InvocationId, ProcedureId};

use crate::procedure_resolver::{ProcedureResolveRequest, ProcedureResolver};

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
    type Procedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure>;
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
pub struct RemoteProcedureDispatcherUnavailable<Procedure> {
    reason: ProcedureDispatchUnavailableReason,
    _procedure: PhantomData<fn() -> Procedure>,
}

impl<Procedure> RemoteProcedureDispatcherUnavailable<Procedure> {
    pub const fn unsupported() -> Self {
        Self {
            reason: ProcedureDispatchUnavailableReason::RemoteDispatchUnsupported,
            _procedure: PhantomData,
        }
    }

    pub const fn unavailable() -> Self {
        Self {
            reason: ProcedureDispatchUnavailableReason::RemoteTransportUnavailable,
            _procedure: PhantomData,
        }
    }

    pub const fn reason(self) -> ProcedureDispatchUnavailableReason {
        self.reason
    }
}

impl<Procedure> ProcedureDispatcher for RemoteProcedureDispatcherUnavailable<Procedure> {
    type Procedure = Procedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure> {
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

pub trait ProcedureRequestResolver {
    fn resolve_request(&self, request: &InvocationRequest) -> AndromedaResult<()>;
}

impl<T> ProcedureRequestResolver for T
where
    T: ProcedureResolver,
{
    fn resolve_request(&self, request: &InvocationRequest) -> AndromedaResult<()> {
        if request.expected_contract_hash != request.procedure.contract_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "Procedure resolution request expected ContractHash must match ProcedureContractRef",
            ));
        }
        if request.catalog_version != request.procedure.catalog_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "Procedure resolution request CatalogVersion must match ProcedureContractRef",
            ));
        }

        let resolve_request = ProcedureResolveRequest::from_contract_ref(request.procedure)
            .map_err(|error| error.into_andromeda_error())?;
        let response = self
            .resolve_procedure(resolve_request.clone())
            .map_err(|error| error.into_andromeda_error())?;

        resolve_request
            .validate_response(&response)
            .map_err(|error| error.into_andromeda_error())
    }
}

#[derive(Clone)]
pub struct SrplDispatcherAdapter<Resolver, Procedure> {
    resolver: Resolver,
    local_dispatcher: Option<Arc<dyn ProcedureDispatcher<Procedure = Procedure> + Send + Sync>>,
}

impl<Resolver, Procedure> SrplDispatcherAdapter<Resolver, Procedure> {
    pub fn new(resolver: Resolver) -> Self {
        Self {
            resolver,
            local_dispatcher: None,
        }
    }

    pub fn with_local_dispatcher<D>(resolver: Resolver, local_dispatcher: D) -> Self
    where
        D: ProcedureDispatcher<Procedure = Procedure> + Send + Sync + 'static,
    {
        Self {
            resolver,
            local_dispatcher: Some(Arc::new(local_dispatcher)),
        }
    }

    pub fn with_shared_local_dispatcher(
        resolver: Resolver,
        local_dispatcher: Arc<dyn ProcedureDispatcher<Procedure = Procedure> + Send + Sync>,
    ) -> Self {
        Self {
            resolver,
            local_dispatcher: Some(local_dispatcher),
        }
    }

    pub fn with_resolver_dispatcher(resolver: Resolver) -> Self
    where
        Resolver: ProcedureDispatcher<Procedure = Procedure> + Clone + Send + Sync + 'static,
    {
        let shared_dispatcher: Arc<dyn ProcedureDispatcher<Procedure = Procedure> + Send + Sync> =
            Arc::new(resolver.clone());
        Self {
            resolver,
            local_dispatcher: Some(shared_dispatcher),
        }
    }
}

impl<Resolver, Procedure> ProcedureDispatcher for SrplDispatcherAdapter<Resolver, Procedure>
where
    Resolver: ProcedureRequestResolver,
{
    type Procedure = Procedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure> {
        request.validate()?;

        let invocation_request = InvocationRequest {
            invocation_id: request.invocation_id,
            procedure: request.procedure,
            expected_binding: request.procedure_binding,
            expected_contract_hash: request.procedure.contract_hash,
            catalog_version: request.procedure.catalog_version,
            structured_parameters: Vec::new(),
        };

        self.resolver.resolve_request(&invocation_request)?;

        if let Some(local_dispatcher) = &self.local_dispatcher {
            return local_dispatcher.dispatch_procedure(request);
        }

        Err(srpl_local_boundary_contract_error(
            invocation_request.procedure.procedure_id,
        ))
    }
}

fn srpl_local_boundary_contract_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "SRPL dispatch boundary requires an explicit local handler for resolved ProcedureId {}; no local Procedure dispatcher is configured",
            procedure_id.get()
        ),
    )
}
