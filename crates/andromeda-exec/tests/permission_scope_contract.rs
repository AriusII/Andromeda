//! Permission scope validation and registry enforcement contracts.

use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};
use andromeda_exec::{
    InvocationContext, LocalProcedure, PermissionScopeValidation, ProcedureHandler,
    ProcedureRegistry, ResultStreamMetadata, validate_dispatch_permissions,
    validate_dispatch_permissions_or_error,
};
use andromeda_observe::TraceId;
use andromeda_srpl::Cardinality;

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
    ) -> andromeda_core::AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.contract(),
            required_permissions: self.required_permissions.clone(),
            result_metadata: self.result_metadata(),
            mutation_payload: vec![1],
            rows_affected: 1,
        })
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

    assert!(err.to_string().contains("permission escalation prevented"));
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

    assert!(err.to_string().contains(ADMIN_PERMISSION));
}

#[test]
fn registry_dispatch_unknown_procedure_fails_closed() {
    let registry = registry_with_reserve_handler();
    let context = InvocationContext::new(TraceId::new(13), permission_vec(&[RESERVE_PERMISSION]));

    let err = registry
        .dispatch(ProcedureId::new(0x9999), context)
        .expect_err("unknown procedure ids must not dispatch");

    assert!(err.to_string().contains("does not contain"));
}
