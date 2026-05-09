pub(crate) use andromeda_catalog_store::{CatalogObjectRef, ObjectKind, QualifiedName};
pub(crate) use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub(crate) use andromeda_exec::dispatch::{
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatcher,
    SrplDispatcherAdapter,
};
pub(crate) use andromeda_exec::{
    InvocationContext, InvocationRequest, LocalProcedure, ResultStreamMetadata,
    SrplProcedureDispatcher,
};
pub(crate) use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
pub(crate) use andromeda_procedure_contract::{
    AccessMode, IsolationPolicy, MultiResultPolicy, PolicyVersion, ProcedureContractBinding,
    ProcedureContractRef, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    ResultStreamCardinality, ResultStreamContract, StatsVersion, TransactionPolicy,
};
pub(crate) use andromeda_procedure_runtime::procedure_resolver::{
    ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse, ProcedureResolver,
    SrplProcedureManifest,
};
pub(crate) use andromeda_srpl_interpreter::SrplIrInterpreter;
pub(crate) use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, Cardinality, ExecutableProcedurePlan,
    SrplCatalogBindingEvidence,
};
pub(crate) use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, InvocationId, ProcedureId,
    ScalarType, TypeDescriptor,
};
pub(crate) use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct MockRejectResolver;

impl ProcedureResolver for MockRejectResolver {
    fn resolve_procedure(
        &self,
        _request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        Err(ProcedureResolveError::InvalidRequest {
            message: "mock resolver not implemented".to_string(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct MockValidResolver {
    response: ProcedureResolveResponse,
}

impl MockValidResolver {
    pub(crate) fn new() -> Self {
        Self {
            response: valid_response(),
        }
    }

    pub(crate) fn with_response(response: ProcedureResolveResponse) -> Self {
        Self { response }
    }
}

impl ProcedureResolver for MockValidResolver {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        request.validate_response(&self.response)?;
        Ok(self.response.clone())
    }
}

#[derive(Clone)]
pub(crate) struct MockLocalDispatcher {
    procedure: LocalProcedure,
}

impl MockLocalDispatcher {
    pub(crate) fn new(procedure: LocalProcedure) -> Self {
        Self { procedure }
    }
}

impl andromeda_procedure_runtime::ProcedureDispatcher for MockLocalDispatcher {
    type Procedure = LocalProcedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure> {
        request.validate()?;
        if request.procedure != self.procedure.contract {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "mock local dispatcher contract mismatch",
            ));
        }
        Ok(self.procedure.clone())
    }
}

pub(crate) fn decision_trace(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
    DecisionTrace {
        trace_id,
        decision,
        reason: "test evidence accepted before transaction creation".to_string(),
    }
}

pub(crate) fn invocation_context(trace_id: TraceId) -> InvocationContext {
    InvocationContext::new(trace_id, Vec::new())
}

fn qualified_name(value: &str) -> QualifiedName {
    QualifiedName::parse(value).expect("valid qualified name")
}

pub(crate) fn contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(1),
    }
}

pub(crate) fn contract_binding() -> ProcedureContractBinding {
    let contract_ref = contract_ref();
    ProcedureContractBinding {
        procedure_id: contract_ref.procedure_id,
        catalog_version: contract_ref.catalog_version,
        contract_hash: contract_ref.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([7; PolicyVersion::LEN]),
    }
}

fn procedure_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(1),
        name: qualified_name("Test.Procedure"),
        kind: ObjectKind::Procedure,
        catalog_version: CatalogVersion::new(1),
    }
}

pub(crate) fn result_stream(stream_id: u64, name: &str) -> ResultStreamContract {
    ResultStreamContract {
        stream_id,
        name: name.to_string(),
        columns: vec![ColumnDescriptor {
            name: "Accepted".to_string(),
            data_type: TypeDescriptor::required(ScalarType::Bool),
            ordinal: 0,
        }],
        cardinality: ResultStreamCardinality::One,
        row_count_exact_required: true,
    }
}

fn valid_manifest(contract_ref: ProcedureContractRef) -> SrplProcedureManifest {
    SrplProcedureManifest {
        contract_ref,
        inputs: vec![ColumnDescriptor {
            name: "InputId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        structured_inputs: Vec::new(),
        result_streams: vec![result_stream(1, "Accepted")],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        required_permissions: vec!["Test.Procedure.Execute".to_string()],
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xD1),
            frame_envelope_hash: ContractHash::test_vector(0xD2),
        },
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["Rejected".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

pub(crate) fn valid_plan() -> ExecutableProcedurePlan {
    let contract_ref = contract_ref();
    ExecutableProcedurePlan {
        procedure_name: qualified_name("Test.Procedure"),
        evidence: SrplCatalogBindingEvidence {
            catalog_version: contract_ref.catalog_version,
            procedure_object: procedure_object(),
            procedure_contract: contract_ref,
            bound_objects: Vec::new(),
        },
        body: BoundSrplBodyPlan {
            operations: vec![BoundSrplOperationPlan::Raise {
                ordinal: 0,
                code: "Rejected".to_string(),
            }],
        },
    }
}

pub(crate) fn valid_response() -> ProcedureResolveResponse {
    let contract_ref = contract_ref();
    ProcedureResolveResponse {
        procedure_id: contract_ref.procedure_id,
        name: qualified_name("Test.Procedure"),
        contract_hash: contract_ref.contract_hash,
        catalog_version: contract_ref.catalog_version,
        contract_ref,
        manifest: valid_manifest(contract_ref),
        plan: valid_plan(),
    }
}

pub(crate) fn srpl_dispatcher(resolver: Arc<dyn ProcedureResolver>) -> SrplProcedureDispatcher {
    SrplProcedureDispatcher::new(resolver, Arc::new(SrplIrInterpreter))
}

pub(crate) fn valid_dispatch_request(trace_id: TraceId) -> ProcedureDispatchRequest {
    ProcedureDispatchRequest {
        invocation_id: InvocationId::new(77),
        procedure: contract_ref(),
        procedure_binding: Some(contract_binding()),
        context: invocation_context(trace_id),
        pre_transaction: PreTransactionDispatchEvidence {
            admission_trace: decision_trace(trace_id, CriticalDecisionKind::ResourceGovernance),
            contract_trace: decision_trace(trace_id, CriticalDecisionKind::ContractValidation),
            authorization_trace: None,
        },
    }
}

pub(crate) fn invocation_request(invocation_id: u64) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract_ref(),
        expected_binding: Some(contract_binding()),
        expected_contract_hash: contract_ref().contract_hash,
        catalog_version: contract_ref().catalog_version,
        structured_parameters: Vec::new(),
    }
}

pub(crate) fn local_procedure() -> LocalProcedure {
    LocalProcedure {
        contract: contract_ref(),
        contract_binding: contract_binding(),
        required_permissions: vec!["Test.Procedure.Execute".to_string()],
        result_metadata: ResultStreamMetadata::exact(1, 1, Cardinality::One, 1),
        mutation_payload: b"local-handler-payload".to_vec(),
        rows_affected: 1,
    }
}
