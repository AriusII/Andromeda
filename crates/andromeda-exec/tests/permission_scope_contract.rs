//! Permission scope validation and registry enforcement contracts.

use andromeda_error::AndromedaErrorKind;

use andromeda_exec::{
    InvocationContext, LocalProcedure, PermissionScopeValidation, PreTransactionDispatchEvidence,
    ProcedureDispatchRequest, ProcedureDispatcher, ProcedureHandler, ProcedureRegistry,
    ResultStreamMetadata, validate_dispatch_permissions, validate_dispatch_permissions_or_error,
};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_procedure_contract::{
    PolicyVersion, ProcedureContractBinding, ProcedureContractRef, StatsVersion,
};
use andromeda_srpl_ir::Cardinality;
use andromeda_types::{CatalogVersion, ContractHash, InvocationId, ProcedureId};

const RESERVE_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x1000);
const RESERVE_PERMISSION: &str = "Inventory.Reserve.Execute";
const QUERY_PERMISSION: &str = "Inventory.Query.Execute";
const ADMIN_PERMISSION: &str = "Admin.Global.Execute";

#[derive(Debug, Clone)]
struct TestProcedureHandler {
    procedure_id: ProcedureId,
    required_permissions: Vec<String>,
}

impl TestProcedureHandler {
    fn new(procedure_id: ProcedureId, required_permissions: &[&str]) -> Self {
        Self {
            procedure_id,
            required_permissions: required_permissions
                .iter()
                .map(|permission| permission.to_string())
                .collect(),
        }
    }
}

impl ProcedureHandler for TestProcedureHandler {
    fn procedure_id(&self) -> ProcedureId {
        self.procedure_id
    }

    fn contract(&self) -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: self.procedure_id,
            contract_hash: ContractHash::test_vector(1),
            catalog_version: CatalogVersion::new(1),
        }
    }

    fn result_metadata(&self) -> ResultStreamMetadata {
        ResultStreamMetadata::exact(1, 1, Cardinality::One, 1)
    }

    fn execute(
        &self,
        _context: InvocationContext,
    ) -> andromeda_error::AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.contract(),
            contract_binding: binding_for(self.contract()),
            required_permissions: self.required_permissions.clone(),
            result_metadata: self.result_metadata(),
            mutation_payload: vec![1],
            rows_affected: 1,
        })
    }
}

fn binding_for(contract: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: contract.procedure_id,
        catalog_version: contract.catalog_version,
        contract_hash: contract.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([1; PolicyVersion::LEN]),
    }
}

fn decision_trace(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
    DecisionTrace {
        trace_id,
        decision,
        reason: "permission scope contract test evidence".to_string(),
    }
}

fn dispatch_request_for(
    contract: ProcedureContractRef,
    trace_id: TraceId,
) -> ProcedureDispatchRequest {
    ProcedureDispatchRequest {
        invocation_id: InvocationId::new(42),
        procedure: contract,
        procedure_binding: Some(binding_for(contract)),
        context: InvocationContext::new(trace_id, permission_vec(&[RESERVE_PERMISSION])),
        pre_transaction: PreTransactionDispatchEvidence {
            admission_trace: decision_trace(trace_id, CriticalDecisionKind::ResourceGovernance),
            contract_trace: decision_trace(trace_id, CriticalDecisionKind::ContractValidation),
            authorization_trace: Some(decision_trace(
                trace_id,
                CriticalDecisionKind::SecurityAuthorization,
            )),
        },
    }
}

fn permission_vec(permissions: &[&str]) -> Vec<String> {
    permissions
        .iter()
        .map(|permission| permission.to_string())
        .collect()
}

fn registry_with_reserve_handler() -> ProcedureRegistry {
    let mut registry = ProcedureRegistry::new();
    registry
        .register(TestProcedureHandler::new(
            RESERVE_PROCEDURE_ID,
            &[RESERVE_PERMISSION, QUERY_PERMISSION],
        ))
        .expect("test handler registration should succeed");
    registry
}

#[test]
fn permission_scope_allows_empty_and_subset_requests() {
    let handler_permissions = permission_vec(&[RESERVE_PERMISSION, QUERY_PERMISSION]);

    assert_eq!(
        validate_dispatch_permissions(&[], &handler_permissions),
        PermissionScopeValidation::Allowed
    );
    assert_eq!(
        validate_dispatch_permissions(&[], &[]),
        PermissionScopeValidation::Allowed
    );
    assert_eq!(
        validate_dispatch_permissions(&permission_vec(&[RESERVE_PERMISSION]), &handler_permissions),
        PermissionScopeValidation::Allowed
    );
}

#[test]
fn permission_scope_denies_first_permission_outside_handler_scope() {
    let result = validate_dispatch_permissions(
        &permission_vec(&[RESERVE_PERMISSION, ADMIN_PERMISSION]),
        &permission_vec(&[RESERVE_PERMISSION, QUERY_PERMISSION]),
    );

    assert_eq!(
        result,
        PermissionScopeValidation::Denied {
            unauthorized_permission: ADMIN_PERMISSION.to_string()
        }
    );

    let extra_permission = validate_dispatch_permissions(
        &permission_vec(&[RESERVE_PERMISSION, QUERY_PERMISSION]),
        &permission_vec(&[RESERVE_PERMISSION]),
    );

    assert_eq!(
        extra_permission,
        PermissionScopeValidation::Denied {
            unauthorized_permission: QUERY_PERMISSION.to_string()
        }
    );
}

#[test]
fn permission_scope_denies_non_empty_request_when_handler_scope_is_empty() {
    let result = validate_dispatch_permissions(&permission_vec(&[RESERVE_PERMISSION]), &[]);

    assert_eq!(
        result,
        PermissionScopeValidation::Denied {
            unauthorized_permission: RESERVE_PERMISSION.to_string()
        }
    );
}

#[test]
fn permission_scope_denial_converts_to_security_error() {
    let err = validate_dispatch_permissions_or_error(
        &permission_vec(&[ADMIN_PERMISSION]),
        &permission_vec(&[RESERVE_PERMISSION]),
    )
    .expect_err("permission escalation must be rejected");

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains(ADMIN_PERMISSION));
    assert!(err.message().contains("handler contract scope"));
    assert!(err.message().contains("permission escalation prevented"));
}

#[test]
fn permission_scope_allows_duplicate_permissions_without_expanding_scope() {
    assert_eq!(
        validate_dispatch_permissions(
            &permission_vec(&[RESERVE_PERMISSION, RESERVE_PERMISSION]),
            &permission_vec(&[RESERVE_PERMISSION]),
        ),
        PermissionScopeValidation::Allowed
    );
    assert_eq!(
        validate_dispatch_permissions(
            &permission_vec(&[RESERVE_PERMISSION]),
            &permission_vec(&[RESERVE_PERMISSION, RESERVE_PERMISSION]),
        ),
        PermissionScopeValidation::Allowed
    );
}

#[test]
fn permission_scope_requires_exact_permission_string_matches() {
    assert_eq!(
        validate_dispatch_permissions(
            &permission_vec(&["namespace:resource:action"]),
            &permission_vec(&["namespace:resource:action"]),
        ),
        PermissionScopeValidation::Allowed
    );

    assert!(
        validate_dispatch_permissions(
            &permission_vec(&["inventory.reserve.execute"]),
            &permission_vec(&["Inventory.Reserve.Execute"]),
        )
        .is_denied()
    );

    assert!(
        validate_dispatch_permissions(&permission_vec(&["admin"]), &permission_vec(&["admi"]),)
            .is_denied()
    );
}

#[test]
fn registry_dispatch_allows_permissions_within_procedure_scope() {
    let registry = registry_with_reserve_handler();
    let context = InvocationContext::new(
        TraceId::new(11),
        permission_vec(&[RESERVE_PERMISSION, QUERY_PERMISSION]),
    );

    let procedure = registry
        .dispatch(RESERVE_PROCEDURE_ID, context)
        .expect("in-scope permissions should dispatch");

    assert_eq!(
        procedure.required_permissions,
        permission_vec(&[RESERVE_PERMISSION, QUERY_PERMISSION])
    );
}

#[test]
fn registry_dispatch_allows_empty_and_subset_context_permissions() {
    let registry = registry_with_reserve_handler();

    for permissions in [Vec::new(), permission_vec(&[RESERVE_PERMISSION])] {
        registry
            .dispatch(
                RESERVE_PROCEDURE_ID,
                InvocationContext::new(TraceId::new(14), permissions),
            )
            .expect("subset caller permissions should dispatch");
    }
}

#[test]
fn registry_dispatch_rejects_context_permission_escalation() {
    let registry = registry_with_reserve_handler();
    let context = InvocationContext::new(
        TraceId::new(12),
        permission_vec(&[RESERVE_PERMISSION, ADMIN_PERMISSION]),
    );

    let err = registry
        .dispatch(RESERVE_PROCEDURE_ID, context)
        .expect_err("out-of-scope caller permission must be rejected");

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains(ADMIN_PERMISSION));
    assert!(err.message().contains("handler contract scope"));
}

#[test]
fn registry_dispatch_unknown_procedure_fails_closed() {
    let registry = registry_with_reserve_handler();
    let context = InvocationContext::new(TraceId::new(13), permission_vec(&[RESERVE_PERMISSION]));

    let err = registry
        .dispatch(ProcedureId::new(0x9999), context)
        .expect_err("unknown procedure ids must not dispatch");

    assert_eq!(err.kind(), AndromedaErrorKind::Execution);
    assert!(err.to_string().contains("does not contain"));
}

#[test]
fn registry_dispatcher_rejects_contract_mismatch_before_handler_execution() {
    let registry = registry_with_reserve_handler();
    let mismatched_contract = ProcedureContractRef {
        procedure_id: RESERVE_PROCEDURE_ID,
        contract_hash: ContractHash::test_vector(99),
        catalog_version: CatalogVersion::new(1),
    };

    let err = ProcedureDispatcher::dispatch_procedure(
        &registry,
        dispatch_request_for(mismatched_contract, TraceId::new(15)),
    )
    .expect_err("dispatch request contract must match the registered cataloged handler");

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message()
            .contains("contract must match registered handler contract")
    );
}
