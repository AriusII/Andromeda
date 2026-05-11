//! P04 exit criterion: two distinct cataloged procedures dispatched generically.
//!
//! Validates that `SrplProcedureDispatcher` correctly routes two procedures
//! with different `ProcedureId`, `ContractHash`, and `CatalogVersion` through
//! a `FakeResolver` backed by pre-registered response records. Each dispatch
//! must succeed independently, and a cross-procedure dispatch (procedure A id
//! with procedure B's `ContractHash`) must be rejected.
//!
//! This test does NOT import `andromeda-inventory-demo` or any business-domain
//! crate. It proves the generic dispatch path using synthetic procedures.

use std::sync::Arc;

use andromeda_catalog_store::{CatalogObjectRef, ObjectKind};
use andromeda_exec::{
    InvocationContext, ProcedureDispatcher, SrplProcedureDispatcher,
    dispatch::{PreTransactionDispatchEvidence, ProcedureDispatchRequest},
};
use andromeda_observability::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_principal::PrincipalId;
use andromeda_procedure_contract::{
    AccessMode, IsolationPolicy, MultiResultPolicy, PolicyVersion, ProcedureContractBinding,
    ProcedureContractRef, ProcedureDeadline, ProcedureDispatchPayload, ProcedureErrorPolicy,
    ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, ResultStreamCardinality,
    ResultStreamContract, StatsVersion, TransactionPolicy,
};
use andromeda_procedure_runtime::procedure_resolver::{
    ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse,
    ProcedureResolveTarget, ProcedureResolver, SrplProcedureManifest,
};
use andromeda_srpl_interpreter::SrplIrInterpreter;
use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, SrplCatalogBindingEvidence,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, InvocationId, ProcedureId,
    ScalarType, TypeDescriptor,
};

// ── Procedure A constants ──────────────────────────────────────────────────

const PROC_A_ID: ProcedureId = ProcedureId::new(101);
const PROC_A_HASH: ContractHash = ContractHash::new([0xAA; ContractHash::LEN]);
const PROC_A_VERSION: CatalogVersion = CatalogVersion::new(5);
const PROC_A_NAME: &str = "Catalog.ProcedureAlpha";

// ── Procedure B constants ──────────────────────────────────────────────────

const PROC_B_ID: ProcedureId = ProcedureId::new(102);
const PROC_B_HASH: ContractHash = ContractHash::new([0xBB; ContractHash::LEN]);
const PROC_B_VERSION: CatalogVersion = CatalogVersion::new(7);
const PROC_B_NAME: &str = "Catalog.ProcedureBeta";

// ── Helper builders ────────────────────────────────────────────────────────

fn qualified_name(value: &str) -> QualifiedName {
    QualifiedName::parse(value).expect("valid qualified name")
}

fn result_stream_many(stream_id: u64, name: &str) -> ResultStreamContract {
    ResultStreamContract {
        stream_id,
        name: name.to_string(),
        columns: vec![ColumnDescriptor {
            name: "Result".to_string(),
            data_type: TypeDescriptor::required(ScalarType::Bool),
            ordinal: 0,
        }],
        cardinality: ResultStreamCardinality::Many,
        row_count_exact_required: false,
    }
}

fn srpl_manifest(contract_ref: ProcedureContractRef) -> SrplProcedureManifest {
    SrplProcedureManifest {
        contract_ref,
        inputs: Vec::new(),
        structured_inputs: Vec::new(),
        result_streams: vec![result_stream_many(1, "Results")],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadOnly,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        required_permissions: vec!["Catalog.Execute".to_string()],
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::new([0xD1; ContractHash::LEN]),
            frame_envelope_hash: ContractHash::new([0xD2; ContractHash::LEN]),
        },
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["NotImplemented".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

fn raise_plan(
    name: QualifiedName,
    contract_ref: ProcedureContractRef,
    catalog_version: CatalogVersion,
) -> ExecutableProcedurePlan {
    ExecutableProcedurePlan {
        procedure_name: name.clone(),
        evidence: SrplCatalogBindingEvidence {
            catalog_version,
            procedure_object: CatalogObjectRef {
                object_id: CatalogObjectId::new(contract_ref.procedure_id.get()),
                name: name.clone(),
                kind: ObjectKind::Procedure,
                catalog_version,
            },
            procedure_contract: contract_ref,
            bound_objects: Vec::new(),
        },
        body: BoundSrplBodyPlan {
            operations: vec![BoundSrplOperationPlan::Raise {
                ordinal: 0,
                code: "NotImplemented".to_string(),
            }],
        },
    }
}

fn resolve_response(
    procedure_id: ProcedureId,
    contract_hash: ContractHash,
    catalog_version: CatalogVersion,
    name: &str,
) -> ProcedureResolveResponse {
    let contract_ref = ProcedureContractRef {
        procedure_id,
        contract_hash,
        catalog_version,
    };
    let qname = qualified_name(name);
    ProcedureResolveResponse {
        procedure_id,
        name: qname.clone(),
        contract_hash,
        catalog_version,
        contract_ref,
        manifest: srpl_manifest(contract_ref),
        plan: raise_plan(qname, contract_ref, catalog_version),
    }
}

fn contract_binding(contract_ref: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: contract_ref.procedure_id,
        catalog_version: contract_ref.catalog_version,
        contract_hash: contract_ref.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new(
            [contract_ref.procedure_id.get() as u8; PolicyVersion::LEN],
        ),
    }
}

fn decision_trace(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
    DecisionTrace {
        trace_id,
        decision,
        reason: "p04 test evidence accepted before dispatch".to_string(),
    }
}

fn dispatch_request(
    contract_ref: ProcedureContractRef,
    invocation_id: u64,
    trace_id: TraceId,
) -> ProcedureDispatchRequest {
    ProcedureDispatchRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract_ref,
        procedure_binding: Some(contract_binding(contract_ref)),
        context: InvocationContext::new(trace_id, Vec::new()),
        pre_transaction: PreTransactionDispatchEvidence {
            admission_trace: decision_trace(trace_id, CriticalDecisionKind::ResourceGovernance),
            contract_trace: decision_trace(trace_id, CriticalDecisionKind::ContractValidation),
            authorization_trace: None,
        },
        payload: ProcedureDispatchPayload::empty(),
        principal: PrincipalId::new(1),
        deadline: ProcedureDeadline::new(std::time::Duration::from_secs(30)),
        idempotency_key: None,
    }
}

// ── DualFakeResolver ───────────────────────────────────────────────────────

/// Resolver pre-loaded with exactly two procedure responses, routing by
/// `ProcedureId`. Unknown procedures return `UnknownProcedure`.
#[derive(Clone)]
struct DualFakeResolver {
    response_a: ProcedureResolveResponse,
    response_b: ProcedureResolveResponse,
}

impl DualFakeResolver {
    fn new() -> Self {
        Self {
            response_a: resolve_response(PROC_A_ID, PROC_A_HASH, PROC_A_VERSION, PROC_A_NAME),
            response_b: resolve_response(PROC_B_ID, PROC_B_HASH, PROC_B_VERSION, PROC_B_NAME),
        }
    }
}

impl ProcedureResolver for DualFakeResolver {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        let response = match &request.target {
            ProcedureResolveTarget::ProcedureId(id) if *id == PROC_A_ID => self.response_a.clone(),
            ProcedureResolveTarget::ProcedureId(id) if *id == PROC_B_ID => self.response_b.clone(),
            _ => {
                return Err(ProcedureResolveError::UnknownProcedure {
                    target: request.target,
                });
            },
        };
        request.validate_response(&response)?;
        Ok(response)
    }
}

fn srpl_dispatcher() -> SrplProcedureDispatcher {
    SrplProcedureDispatcher::new(
        Arc::new(DualFakeResolver::new()),
        Arc::new(SrplIrInterpreter),
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn p04_procedure_a_dispatches_successfully_via_generic_srpl_path() {
    let dispatcher = srpl_dispatcher();
    let contract_ref_a = ProcedureContractRef {
        procedure_id: PROC_A_ID,
        contract_hash: PROC_A_HASH,
        catalog_version: PROC_A_VERSION,
    };
    let request = dispatch_request(contract_ref_a, 201, TraceId::new(11));

    let procedure = dispatcher
        .dispatch_procedure(request)
        .expect("procedure A dispatch must succeed");

    assert_eq!(
        procedure.contract.procedure_id, PROC_A_ID,
        "dispatched procedure must carry procedure A contract id"
    );
    assert_eq!(procedure.contract.contract_hash, PROC_A_HASH);
    assert_eq!(procedure.contract.catalog_version, PROC_A_VERSION);
    assert_eq!(
        procedure.rows_affected, 0,
        "ReadOnly procedure must not affect rows"
    );
}

#[test]
fn p04_procedure_b_dispatches_successfully_via_generic_srpl_path() {
    let dispatcher = srpl_dispatcher();
    let contract_ref_b = ProcedureContractRef {
        procedure_id: PROC_B_ID,
        contract_hash: PROC_B_HASH,
        catalog_version: PROC_B_VERSION,
    };
    let request = dispatch_request(contract_ref_b, 202, TraceId::new(22));

    let procedure = dispatcher
        .dispatch_procedure(request)
        .expect("procedure B dispatch must succeed");

    assert_eq!(
        procedure.contract.procedure_id, PROC_B_ID,
        "dispatched procedure must carry procedure B contract id"
    );
    assert_eq!(procedure.contract.contract_hash, PROC_B_HASH);
    assert_eq!(procedure.contract.catalog_version, PROC_B_VERSION);
    assert_eq!(
        procedure.rows_affected, 0,
        "ReadOnly procedure must not affect rows"
    );
}

#[test]
fn p04_two_procedures_have_distinct_contract_ids_after_independent_dispatch() {
    let dispatcher = srpl_dispatcher();

    let contract_ref_a = ProcedureContractRef {
        procedure_id: PROC_A_ID,
        contract_hash: PROC_A_HASH,
        catalog_version: PROC_A_VERSION,
    };
    let contract_ref_b = ProcedureContractRef {
        procedure_id: PROC_B_ID,
        contract_hash: PROC_B_HASH,
        catalog_version: PROC_B_VERSION,
    };

    let procedure_a = dispatcher
        .dispatch_procedure(dispatch_request(contract_ref_a, 301, TraceId::new(31)))
        .expect("procedure A dispatch must succeed");

    let procedure_b = dispatcher
        .dispatch_procedure(dispatch_request(contract_ref_b, 302, TraceId::new(32)))
        .expect("procedure B dispatch must succeed");

    assert_ne!(
        procedure_a.contract.procedure_id, procedure_b.contract.procedure_id,
        "two distinct procedures must produce distinct contract procedure ids"
    );
    assert_ne!(
        procedure_a.contract.contract_hash, procedure_b.contract.contract_hash,
        "two distinct procedures must produce distinct contract hashes"
    );
}

#[test]
fn p04_cross_contract_hash_dispatch_is_rejected_fail_closed() {
    let dispatcher = srpl_dispatcher();

    // Use procedure A's ProcedureId but procedure B's ContractHash — must be
    // rejected by validate_response (contract hash mismatch).
    let mismatched_contract = ProcedureContractRef {
        procedure_id: PROC_A_ID,
        contract_hash: PROC_B_HASH, // wrong hash for proc A
        catalog_version: PROC_A_VERSION,
    };
    let mismatched_binding = ProcedureContractBinding {
        procedure_id: PROC_A_ID,
        catalog_version: PROC_A_VERSION,
        contract_hash: PROC_B_HASH,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([0xAB; PolicyVersion::LEN]),
    };
    let request = ProcedureDispatchRequest {
        invocation_id: InvocationId::new(999),
        procedure: mismatched_contract,
        procedure_binding: Some(mismatched_binding),
        context: InvocationContext::new(TraceId::new(99), Vec::new()),
        pre_transaction: PreTransactionDispatchEvidence {
            admission_trace: decision_trace(
                TraceId::new(99),
                CriticalDecisionKind::ResourceGovernance,
            ),
            contract_trace: decision_trace(
                TraceId::new(99),
                CriticalDecisionKind::ContractValidation,
            ),
            authorization_trace: None,
        },
        payload: ProcedureDispatchPayload::empty(),
        principal: PrincipalId::new(1),
        deadline: ProcedureDeadline::new(std::time::Duration::from_secs(30)),
        idempotency_key: None,
    };

    let err = dispatcher
        .dispatch_procedure(request)
        .expect_err("cross-contract dispatch must fail-closed");

    // The error must be a contract-class error (hash mismatch detected at
    // resolver validate_response boundary).
    use andromeda_error::AndromedaErrorKind;
    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Contract,
        "cross-contract hash dispatch must produce a Contract-kind error, got: {:?}",
        err.kind()
    );
}

#[test]
fn p04_pre_transaction_evidence_is_present_on_successful_dispatch_request() {
    // Proves that the dispatched request carries non-empty pre-transaction
    // evidence traces. This is a structural property check, not an execution
    // check.
    let contract_ref_a = ProcedureContractRef {
        procedure_id: PROC_A_ID,
        contract_hash: PROC_A_HASH,
        catalog_version: PROC_A_VERSION,
    };
    let trace_id = TraceId::new(77);
    let request = dispatch_request(contract_ref_a, 777, trace_id);

    // Evidence traces must carry the invocation's trace id.
    assert_eq!(request.pre_transaction.admission_trace.trace_id, trace_id);
    assert_eq!(request.pre_transaction.contract_trace.trace_id, trace_id);

    // Admission and contract traces must carry the expected decision kinds.
    assert_eq!(
        request.pre_transaction.admission_trace.decision,
        CriticalDecisionKind::ResourceGovernance
    );
    assert_eq!(
        request.pre_transaction.contract_trace.decision,
        CriticalDecisionKind::ContractValidation
    );

    // Request itself must be valid before dispatch.
    request
        .validate()
        .expect("dispatch request must pass validate()");
}
