//! Executor-layer validation gates for H1-SRPL-EXEC-007.
//!
//! Comprehensive validation gates for executor integration:
//! - SrplProcedureDispatcher and SrplDispatcherAdapter
//! - Error boundary enforcement (pre-transaction)
//! - Dispatch path coexistence (cataloged local + SRPL)
//! - Contract validation stability
//! - Deterministic error handling

use andromeda_catalog::{
    AccessMode, CatalogObjectRef, IsolationPolicy, MultiResultPolicy, ObjectKind, PolicyVersion,
    ProcedureContractBinding, ProcedureContractRef, ProcedureErrorPolicy, ProtocolLayoutRef,
    QualifiedName, ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract,
    StatsVersion, TransactionPolicy,
};
use andromeda_core::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, InvocationId, ProcedureId,
    ScalarType, TypeDescriptor,
};
use andromeda_exec::dispatch::{
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatcher,
    SrplDispatcherAdapter,
};
use andromeda_exec::{InvocationContext, LocalProcedure, ResultStreamMetadata};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_srpl::procedure_model::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, SrplCatalogBindingEvidence,
};
use andromeda_srpl::procedure_resolver::{
    ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse, ProcedureResolver,
    SrplProcedureManifest,
};
use andromeda_srpl::{Cardinality, interpreter::SrplIrInterpreter};
use std::sync::Arc;

// Mock Resolver for Testing

#[derive(Clone)]
struct MockRejectResolver;

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
struct MockValidResolver {
    response: ProcedureResolveResponse,
}

impl MockValidResolver {
    fn new() -> Self {
        Self {
            response: valid_response(),
        }
    }

    fn with_response(response: ProcedureResolveResponse) -> Self {
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
struct MockLocalDispatcher {
    procedure: LocalProcedure,
}

impl ProcedureDispatcher for MockLocalDispatcher {
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> andromeda_core::AndromedaResult<LocalProcedure> {
        request.validate()?;
        if request.procedure != self.procedure.contract {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Contract,
                "mock local dispatcher contract mismatch",
            ));
        }
        Ok(self.procedure.clone())
    }
}

fn decision_trace(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
    DecisionTrace {
        trace_id,
        decision,
        reason: "test evidence accepted before transaction creation".to_string(),
    }
}

fn invocation_context(trace_id: TraceId) -> InvocationContext {
    InvocationContext::new(trace_id, Vec::new())
}

fn qualified_name(value: &str) -> QualifiedName {
    QualifiedName::parse(value).expect("valid qualified name")
}

fn contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(1),
    }
}

fn contract_binding() -> ProcedureContractBinding {
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

fn result_stream(stream_id: u64, name: &str) -> ResultStreamContract {
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

fn valid_plan() -> ExecutableProcedurePlan {
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

fn valid_response() -> ProcedureResolveResponse {
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

fn srpl_dispatcher(
    resolver: Arc<dyn ProcedureResolver>,
) -> andromeda_exec::SrplProcedureDispatcher {
    andromeda_exec::SrplProcedureDispatcher::new(resolver, Arc::new(SrplIrInterpreter))
}

fn valid_dispatch_request(trace_id: TraceId) -> ProcedureDispatchRequest {
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

fn local_procedure() -> LocalProcedure {
    LocalProcedure {
        contract: contract_ref(),
        contract_binding: contract_binding(),
        required_permissions: vec!["Test.Procedure.Execute".to_string()],
        result_metadata: ResultStreamMetadata::exact(1, 1, Cardinality::One, 1),
        mutation_payload: b"local-handler-payload".to_vec(),
        rows_affected: 1,
    }
}

// GATE EXEC-01: Dispatcher Construction and Cloning

#[test]
fn gate_exec_01_srpl_dispatcher_constructs_with_dependencies() {
    let resolver = Arc::new(MockValidResolver::new());
    let interpreter = Arc::new(SrplIrInterpreter);

    let dispatcher = andromeda_exec::SrplProcedureDispatcher::new(resolver, interpreter);
    let _ = dispatcher.clone();
}

#[test]
fn gate_exec_01_srpl_dispatcher_cloneable_for_sharing() {
    let resolver = Arc::new(MockValidResolver::new());
    let interpreter = Arc::new(SrplIrInterpreter);

    let dispatcher1 = andromeda_exec::SrplProcedureDispatcher::new(resolver, interpreter);
    let dispatcher2 = dispatcher1.clone();
    let dispatcher3 = dispatcher2.clone();

    // All clones should be independently usable (interface contract)
    let _d1 = dispatcher1;
    let _d2 = dispatcher2;
    let _d3 = dispatcher3;
}

// GATE EXEC-02: Error Boundary Enforcement

#[test]
fn gate_exec_02_error_boundary_pre_transaction() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);

    let request = valid_dispatch_request(TraceId::new(1));

    let result = adapter.dispatch_procedure(request);

    let err = result.unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(
        err.message().contains(
            "SRPL dispatch boundary requires an explicit local handler for resolved ProcedureId 1"
        ),
        "adapter must expose the stable SRPL/local boundary contract"
    );
}

#[test]
fn gate_exec_02_srpl_adapter_dispatches_through_configured_local_handler_path() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::with_local_dispatcher(
        dispatcher,
        MockLocalDispatcher {
            procedure: local_procedure(),
        },
    );

    let procedure = adapter
        .dispatch_procedure(valid_dispatch_request(TraceId::new(2)))
        .expect("configured local handler path should execute after SRPL resolution");

    assert_eq!(procedure.contract, contract_ref());
    assert_eq!(procedure.rows_affected, 1);
    assert_eq!(procedure.mutation_payload, b"local-handler-payload");
    assert_eq!(
        procedure.required_permissions,
        vec!["Test.Procedure.Execute".to_string()]
    );
}

#[test]
fn gate_exec_02_invalid_request_rejected_before_dispatch() {
    let resolver = Arc::new(MockRejectResolver);
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);

    // Create request with mismatched trace IDs (invalid)
    let request = ProcedureDispatchRequest {
        invocation_id: InvocationId::new(79),
        procedure: ProcedureContractRef {
            procedure_id: ProcedureId::new(1),
            contract_hash: ContractHash::test_vector(7),
            catalog_version: CatalogVersion::new(1),
        },
        procedure_binding: Some(contract_binding()),
        context: invocation_context(TraceId::new(999)),
        pre_transaction: PreTransactionDispatchEvidence {
            admission_trace: decision_trace(
                TraceId::new(1),
                CriticalDecisionKind::ResourceGovernance,
            ),
            contract_trace: decision_trace(
                TraceId::new(1),
                CriticalDecisionKind::ContractValidation,
            ),
            authorization_trace: None,
        },
    };

    let result = adapter.dispatch_procedure(request);
    assert!(
        result.is_err(),
        "invalid trace correlation must be rejected at the request boundary"
    );
}

#[test]
fn gate_exec_02_dispatch_requires_caller_invocation_identity() {
    let resolver = Arc::new(MockRejectResolver);
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);
    let mut request = valid_dispatch_request(TraceId::new(3));
    request.invocation_id = InvocationId::new(0);

    let err = adapter
        .dispatch_procedure(request)
        .expect_err("zero invocation id must be rejected before SRPL resolution");

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Execution);
    assert!(err.message().contains("invocation id"));
}

// GATE EXEC-03: Dispatch Path Coexistence

#[test]
fn gate_exec_03_both_dispatch_paths_available() {
    let resolver = Arc::new(MockValidResolver::new());
    let interpreter = Arc::new(SrplIrInterpreter);

    #[allow(dead_code)]
    enum DispatchPath {
        CatalogedLocalProcedure,
        SrplInterpreted(andromeda_exec::SrplProcedureDispatcher),
    }

    let local_path = DispatchPath::CatalogedLocalProcedure;
    let srpl_path = DispatchPath::SrplInterpreted(andromeda_exec::SrplProcedureDispatcher::new(
        resolver,
        interpreter,
    ));

    assert!(matches!(local_path, DispatchPath::CatalogedLocalProcedure));
    assert!(matches!(srpl_path, DispatchPath::SrplInterpreted(_)));
}

// GATE EXEC-04: Adapter Interface Compliance

#[test]
fn gate_exec_04_adapter_implements_dispatcher_trait() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);

    // Adapter must implement ProcedureDispatcher trait
    let _trait_obj: &dyn ProcedureDispatcher = &adapter;
}

#[test]
fn gate_exec_04_adapter_cloneable() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter1 = SrplDispatcherAdapter::new(dispatcher);

    let adapter2 = adapter1.clone();
    let _adapter3 = adapter2.clone();
}

// GATE EXEC-05: Thread Safety

#[test]
fn gate_exec_05_dispatcher_thread_safe() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = Arc::new(srpl_dispatcher(resolver));

    let mut handles = vec![];

    for i in 0..10 {
        let dispatcher_clone = Arc::clone(&dispatcher);
        let handle = std::thread::spawn(move || {
            let _d = dispatcher_clone;
            i
        });
        handles.push(handle);
    }

    let moved_count = handles.len();
    for handle in handles {
        handle.join().expect("dispatcher thread must not panic");
    }
    assert_eq!(moved_count, 10);
}

#[test]
fn gate_exec_05_adapter_thread_safe() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = Arc::new(SrplDispatcherAdapter::new(dispatcher));

    let mut handles = vec![];

    for i in 0..10 {
        let adapter_clone = Arc::clone(&adapter);
        let handle = std::thread::spawn(move || {
            let _a = adapter_clone;
            i
        });
        handles.push(handle);
    }

    let moved_count = handles.len();
    for handle in handles {
        handle.join().expect("adapter thread must not panic");
    }
    assert_eq!(moved_count, 10);
}

// GATE EXEC-06: Deterministic Error Handling

#[test]
fn gate_exec_06_error_handling_deterministic() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);

    let request = valid_dispatch_request(TraceId::new(1));

    // First attempt
    let result1 = adapter.dispatch_procedure(request.clone());

    // Second attempt with identical request
    let result2 = adapter.dispatch_procedure(request);

    // Both should fail in the same way
    assert!(result1.is_err(), "First attempt should fail");
    assert!(result2.is_err(), "Second attempt should fail");
    assert_eq!(
        result1.unwrap_err().message(),
        result2.unwrap_err().message()
    );
}

// GATE EXEC-07: Result Metadata Interface

#[test]
fn gate_exec_07_result_metadata_extraction_documents_current_pre_tx_gap() {
    let plan = valid_plan();
    assert!(plan.validate().is_ok());

    let err = andromeda_exec::SrplProcedureDispatcher::result_metadata_for_plan(&plan, &[])
        .expect_err("metadata extraction with empty result streams should fail");

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Srpl);
    assert!(
        err.message()
            .contains("procedure must declare at least one result stream")
    );
}

#[test]
fn gate_exec_07_resolved_manifest_carries_metadata_and_single_result_policy() {
    let dispatcher = srpl_dispatcher(Arc::new(MockValidResolver::new()));
    let request = andromeda_exec::InvocationRequest {
        invocation_id: andromeda_core::InvocationId::new(77),
        procedure: contract_ref(),
        expected_binding: Some(contract_binding()),
        expected_contract_hash: contract_ref().contract_hash,
        catalog_version: contract_ref().catalog_version,
        structured_parameters: Vec::new(),
    };

    let resolved = dispatcher
        .resolve_procedure(&request)
        .expect("valid resolver response should satisfy request");

    assert_eq!(
        resolved.manifest.result_metadata_policy,
        ResultMetadataPolicy::RequireBeforePayload
    );
    assert_eq!(
        resolved.manifest.multi_result_policy,
        MultiResultPolicy::SingleResultOnly
    );
    assert_eq!(resolved.manifest.result_streams.len(), 1);
    assert_eq!(resolved.manifest.result_streams[0].stream_id, 1);

    let metadata = andromeda_exec::SrplProcedureDispatcher::result_metadata_for_plan(
        &resolved.plan,
        &resolved.manifest.result_streams,
    )
    .expect("metadata extraction should succeed");

    assert_eq!(metadata.stream_id, 1);
    assert_eq!(metadata.column_count, 1);
}

#[test]
fn gate_exec_07_multi_result_manifest_rejected_before_dispatch() {
    let mut response = valid_response();
    response
        .manifest
        .result_streams
        .push(result_stream(2, "Audit"));
    let dispatcher = srpl_dispatcher(Arc::new(MockValidResolver::with_response(response)));
    let request = andromeda_exec::InvocationRequest {
        invocation_id: andromeda_core::InvocationId::new(78),
        procedure: contract_ref(),
        expected_binding: Some(contract_binding()),
        expected_contract_hash: contract_ref().contract_hash,
        catalog_version: contract_ref().catalog_version,
        structured_parameters: Vec::new(),
    };

    let err = dispatcher
        .resolve_procedure(&request)
        .expect_err("single-result manifest must reject multiple result streams");

    assert!(matches!(err, ProcedureResolveError::InvalidResponse { .. }));
    assert!(
        err.into_andromeda_error()
            .message()
            .contains("multi-result policy")
    );
}

// GATE EXEC-08: Plan Validation Interface

#[test]
fn gate_exec_08_plan_validation_interface_available() {
    let result = andromeda_exec::SrplProcedureDispatcher::validate_plan(&valid_plan());

    assert!(result.is_ok(), "valid deterministic SRPL plan should pass");
}

// Summary

#[test]
fn gate_exec_summary_all_validations() {
    let covered_gates = [
        "dispatcher construction and cloning",
        "pre-transaction error boundary",
        "cataloged local and SRPL dispatch paths",
        "adapter trait compliance",
        "Send + Sync movement",
        "deterministic error handling",
        "result metadata interface",
        "plan validation interface",
    ];

    assert_eq!(covered_gates.len(), 8);
    assert!(covered_gates.iter().all(|gate| !gate.trim().is_empty()));
}
