use std::{collections::HashMap, sync::Arc};

use andromeda_admission::InvocationContext;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::ProcedureContractRef;
use andromeda_procedure_runtime::{ProcedureDispatchRequest, ProcedureDispatcher};
use andromeda_result_stream::ResultStreamMetadata;
use andromeda_types::ProcedureId;

use crate::LocalProcedure;

/// Executable Procedure adapter for registry-backed local dispatch.
///
/// Callers must invoke a handler only after admission, surface authorization,
/// permission checks, and catalog contract validation have accepted the
/// invocation. The handler does not own transaction allocation, WAL dispatch,
/// remote dispatch, or terminal completion mapping.
pub trait ProcedureHandler {
    /// Stable Procedure identifier used for request matching and registry keys.
    fn procedure_id(&self) -> ProcedureId;

    /// Contract metadata bound to the executable Procedure implementation.
    fn contract(&self) -> ProcedureContractRef;

    /// Result stream metadata that must be validated before payload emission.
    fn result_metadata(&self) -> ResultStreamMetadata;

    /// Execute an already-admitted invocation into the local runtime result
    /// shape.
    fn execute(&self, context: InvocationContext) -> AndromedaResult<LocalProcedure>;
}

/// Registry for local executable Procedure handlers.
///
/// The registry is keyed by [`ProcedureId`], not by raw names, SQL text, remote
/// addresses, or transport method names. Callers must reach this boundary only
/// after admission, surface authorization, permission checks, and catalog
/// contract validation have accepted the invocation.
#[derive(Clone, Default)]
pub struct ProcedureRegistry {
    handlers: HashMap<ProcedureId, Arc<dyn ProcedureHandler + Send + Sync>>,
}

impl ProcedureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn register<H>(&mut self, handler: H) -> AndromedaResult<()>
    where
        H: ProcedureHandler + Send + Sync + 'static,
    {
        self.register_arc(Arc::new(handler))
    }

    pub fn register_arc(
        &mut self,
        handler: Arc<dyn ProcedureHandler + Send + Sync>,
    ) -> AndromedaResult<()> {
        let procedure_id = handler.procedure_id();

        if self.handlers.contains_key(&procedure_id) {
            return Err(duplicate_procedure_error(procedure_id));
        }

        validate_handler_registration(handler.as_ref())?;
        self.handlers.insert(procedure_id, handler);
        Ok(())
    }

    pub fn contains(&self, procedure_id: ProcedureId) -> bool {
        self.handlers.contains_key(&procedure_id)
    }

    pub fn lookup(
        &self,
        procedure_id: ProcedureId,
    ) -> Option<Arc<dyn ProcedureHandler + Send + Sync>> {
        self.handlers.get(&procedure_id).cloned()
    }

    /// Dispatch an already-admitted local Procedure invocation.
    ///
    /// This method accepts only a canonical [`ProcedureId`]. It does not
    /// perform remote dispatch, transaction allocation, WAL emission, or
    /// terminal completion mapping.
    pub fn dispatch(
        &self,
        procedure_id: ProcedureId,
        context: InvocationContext,
    ) -> AndromedaResult<LocalProcedure> {
        let handler = self
            .lookup(procedure_id)
            .ok_or_else(|| unknown_procedure_error(procedure_id))?;
        let procedure = handler.execute(context.clone())?;
        validate_dispatch_result(procedure_id, handler.as_ref(), &procedure, &context)?;
        Ok(procedure)
    }
}

impl ProcedureDispatcher for ProcedureRegistry {
    type Procedure = LocalProcedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure> {
        request.validate()?;

        let procedure_id = request.procedure.procedure_id;
        let handler = self
            .lookup(procedure_id)
            .ok_or_else(|| unknown_procedure_error(procedure_id))?;

        if handler.contract() != request.procedure {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "dispatch request contract must match registered handler contract before execution",
            ));
        }
        let request_binding = request.procedure_binding.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "dispatch request requires ProcedureContractBinding before handler execution",
            )
        })?;

        let procedure = handler.execute(request.context.clone())?;
        if procedure.contract_binding != request_binding {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "dispatched ProcedureContractBinding must match pre-transaction dispatch binding",
            ));
        }
        validate_dispatch_result(procedure_id, handler.as_ref(), &procedure, &request.context)?;
        Ok(procedure)
    }
}

fn validate_handler_registration(
    handler: &(dyn ProcedureHandler + Send + Sync),
) -> AndromedaResult<()> {
    let procedure_id = handler.procedure_id();

    if procedure_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "registered Procedure id must not be zero",
        ));
    }

    let contract = handler.contract();
    contract.validate()?;

    if contract.procedure_id != procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "registered Procedure handler id must match its contract Procedure id",
        ));
    }

    handler.result_metadata().validate_before_payload()?;
    Ok(())
}

fn validate_dispatch_result(
    procedure_id: ProcedureId,
    handler: &(dyn ProcedureHandler + Send + Sync),
    procedure: &LocalProcedure,
    context: &InvocationContext,
) -> AndromedaResult<()> {
    procedure.validate()?;

    if procedure.contract != handler.contract() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "dispatched Procedure contract must match registered handler contract",
        ));
    }

    if procedure.contract.procedure_id != procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "dispatched Procedure id must match requested Procedure id",
        ));
    }

    validate_dispatch_permissions_or_error(
        &context.granted_permissions,
        &procedure.required_permissions,
    )
}

fn validate_dispatch_permissions_or_error(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
) -> AndromedaResult<()> {
    if request_permissions.is_empty() {
        return Ok(());
    }

    for request_permission in request_permissions {
        if !handler_contract_permissions
            .iter()
            .any(|handler_permission| handler_permission == request_permission)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                format!(
                    "requested permission \"{}\" exceeds handler contract scope; permission escalation prevented",
                    request_permission
                ),
            ));
        }
    }

    Ok(())
}

fn duplicate_procedure_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "ProcedureRegistry already contains ProcedureId {}",
            procedure_id.get()
        ),
    )
}

fn unknown_procedure_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Execution,
        format!(
            "ProcedureRegistry does not contain ProcedureId {}",
            procedure_id.get()
        ),
    )
}

#[cfg(test)]
mod tests {
    use andromeda_admission::InvocationContext;
    use andromeda_error::{AndromedaErrorKind, AndromedaResult};
    use andromeda_observability::{CriticalDecisionKind, DecisionTrace, TraceId};
    use andromeda_procedure_contract::{
        PolicyVersion, ProcedureContractBinding, ProcedureContractRef, StatsVersion,
    };
    use andromeda_procedure_runtime::{
        PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatcher,
        RemoteProcedureDispatcherUnavailable,
    };
    use andromeda_result_stream::ResultStreamMetadata;
    use andromeda_srpl_ir::Cardinality;
    use andromeda_types::{CatalogVersion, ContractHash, InvocationId, ProcedureId};

    use crate::LocalProcedure;

    use super::{ProcedureHandler, ProcedureRegistry};

    #[derive(Debug, Clone, Copy)]
    struct FakeProcedureHandler {
        contract: ProcedureContractRef,
        result_contract: ProcedureContractRef,
        rows_affected: u64,
        invalid_payload_shape: bool,
    }

    impl ProcedureHandler for FakeProcedureHandler {
        fn procedure_id(&self) -> ProcedureId {
            self.contract.procedure_id
        }

        fn contract(&self) -> ProcedureContractRef {
            self.contract
        }

        fn result_metadata(&self) -> ResultStreamMetadata {
            ResultStreamMetadata::exact(7, 1, Cardinality::One, 1)
        }

        fn execute(&self, _context: InvocationContext) -> AndromedaResult<LocalProcedure> {
            Ok(LocalProcedure {
                contract: self.result_contract,
                contract_binding: binding_for(self.result_contract),
                required_permissions: vec!["procedure.demo.execute".to_string()],
                result_metadata: self.result_metadata(),
                mutation_payload: if self.invalid_payload_shape {
                    Vec::new()
                } else {
                    vec![1]
                },
                rows_affected: self.rows_affected,
            })
        }
    }

    #[test]
    fn procedure_handler_exposes_metadata_and_local_result_shape() {
        let contract = contract(42);
        let handler = FakeProcedureHandler {
            contract,
            result_contract: contract,
            rows_affected: 1,
            invalid_payload_shape: false,
        };

        assert_eq!(handler.procedure_id(), ProcedureId::new(42));
        assert_eq!(handler.contract(), contract);
        assert_eq!(handler.result_metadata().row_count_exact, Some(1));

        let procedure = handler
            .execute(context())
            .expect("fake handler should produce a local procedure");

        procedure
            .validate()
            .expect("fake handler result should satisfy LocalProcedure shape");

        assert_eq!(procedure.contract, contract);
        assert_eq!(procedure.rows_affected, 1);
    }

    #[test]
    fn registry_registers_and_dispatches_handler() {
        let mut registry = ProcedureRegistry::new();
        registry.register(handler(42)).unwrap();

        assert_eq!(registry.len(), 1);
        assert!(registry.contains(ProcedureId::new(42)));

        let procedure = registry.dispatch(ProcedureId::new(42), context()).unwrap();
        assert_eq!(procedure.contract.procedure_id, ProcedureId::new(42));
        assert_eq!(procedure.rows_affected, 1);
    }

    #[test]
    fn procedure_dispatcher_invokes_registry_handler_after_pre_transaction_evidence() {
        let mut registry = ProcedureRegistry::new();
        let handler = handler(42);
        let request = dispatch_request(handler.contract());
        registry.register(handler).unwrap();

        let procedure = ProcedureDispatcher::dispatch_procedure(&registry, request).unwrap();

        assert_eq!(procedure.contract.procedure_id, ProcedureId::new(42));
        assert_eq!(procedure.rows_affected, 1);
    }

    #[test]
    fn procedure_dispatcher_rejects_invalid_evidence_before_registry_lookup() {
        let registry = ProcedureRegistry::new();
        let mut request = dispatch_request(contract(42));
        request.pre_transaction.contract_trace.decision = CriticalDecisionKind::WalAppend;

        let error = ProcedureDispatcher::dispatch_procedure(&registry, request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn procedure_dispatcher_rejects_contract_mismatch_before_handler_execution() {
        let mut registry = ProcedureRegistry::new();
        registry.register(handler(42)).unwrap();
        let mut mismatched_contract = contract(42);
        mismatched_contract.contract_hash = ContractHash::test_vector(99);
        let request = dispatch_request(mismatched_contract);

        let error = ProcedureDispatcher::dispatch_procedure(&registry, request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn remote_procedure_dispatcher_unavailable_returns_typed_transport_error() {
        let dispatcher = RemoteProcedureDispatcherUnavailable::<LocalProcedure>::unsupported();
        let request = dispatch_request(contract(42));

        let error = dispatcher.dispatch_procedure(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Transport);
    }

    #[test]
    fn registry_rejects_duplicate_procedure_id() {
        let mut registry = ProcedureRegistry::new();
        registry.register(handler(42)).unwrap();

        let duplicate = registry.register(handler(42)).unwrap_err();
        assert_eq!(duplicate.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn registry_returns_explicit_unknown_procedure_error() {
        let registry = ProcedureRegistry::new();

        let error = registry
            .dispatch(ProcedureId::new(404), context())
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Execution);
    }

    #[test]
    fn registry_rejects_handler_contract_mismatch() {
        let mut registry = ProcedureRegistry::new();
        let mut handler = handler(42);
        handler.contract = ProcedureContractRef {
            procedure_id: ProcedureId::new(42),
            contract_hash: ContractHash::test_vector(7),
            catalog_version: CatalogVersion::new(0),
        };

        let error = registry.register(handler).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn registry_rejects_dispatch_result_contract_mismatch() {
        let mut registry = ProcedureRegistry::new();
        let mut handler = handler(42);
        handler.result_contract = contract(43);
        registry.register(handler).unwrap();

        let error = registry
            .dispatch(ProcedureId::new(42), context())
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn registry_rejects_invalid_dispatch_result_shape() {
        let mut registry = ProcedureRegistry::new();
        let mut handler = handler(42);
        handler.invalid_payload_shape = true;
        registry.register(handler).unwrap();

        let error = registry
            .dispatch(ProcedureId::new(42), context())
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Execution);
    }

    fn contract(procedure_id: u64) -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: ProcedureId::new(procedure_id),
            contract_hash: ContractHash::test_vector(procedure_id as u8),
            catalog_version: CatalogVersion::new(3),
        }
    }

    fn binding_for(contract: ProcedureContractRef) -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: contract.procedure_id,
            catalog_version: contract.catalog_version,
            contract_hash: contract.contract_hash,
            stats_version: StatsVersion::new(1),
            policy_version: PolicyVersion::new(
                [contract.procedure_id.get() as u8; PolicyVersion::LEN],
            ),
        }
    }

    fn handler(procedure_id: u64) -> FakeProcedureHandler {
        let contract = contract(procedure_id);
        FakeProcedureHandler {
            contract,
            result_contract: contract,
            rows_affected: 1,
            invalid_payload_shape: false,
        }
    }

    fn context() -> InvocationContext {
        InvocationContext::new(TraceId::new(9), vec!["procedure.demo.execute".to_string()])
    }

    fn decision(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
        DecisionTrace {
            trace_id,
            decision,
            reason: format!("{decision:?} accepted before transaction creation"),
        }
    }

    fn pre_transaction_evidence(trace_id: TraceId) -> PreTransactionDispatchEvidence {
        PreTransactionDispatchEvidence {
            admission_trace: decision(trace_id, CriticalDecisionKind::ResourceGovernance),
            contract_trace: decision(trace_id, CriticalDecisionKind::ContractValidation),
            authorization_trace: Some(decision(
                trace_id,
                CriticalDecisionKind::SecurityAuthorization,
            )),
        }
    }

    fn dispatch_request(procedure: ProcedureContractRef) -> ProcedureDispatchRequest {
        let context = context();
        ProcedureDispatchRequest {
            invocation_id: InvocationId::new(700),
            procedure,
            procedure_binding: Some(binding_for(procedure)),
            pre_transaction: pre_transaction_evidence(context.trace_id),
            context,
        }
    }
}
