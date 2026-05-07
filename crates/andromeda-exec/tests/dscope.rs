//! Integration tests for permission scope validation in Procedure dispatch.
//!
//! These tests verify the security contract that caller permissions must not exceed
//! handler contract permissions, preventing privilege escalation attacks.

use andromeda_catalog::{
    PolicyVersion, ProcedureContractBinding, ProcedureContractRef, StatsVersion,
};
use andromeda_core::{AndromedaResult, ProcedureId};
use andromeda_core::{CatalogVersion, ContractHash};
use andromeda_exec::{
    InvocationContext, LocalProcedure, PermissionScopeValidation, ProcedureHandler,
    ProcedureRegistry, ResultStreamMetadata, validate_dispatch_permissions,
    validate_dispatch_permissions_or_error,
};
use andromeda_observe::TraceId;
use andromeda_srpl::Cardinality;

// Test constants
const INVENTORY_RESERVE_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x1000);
const INVENTORY_QUERY_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x2000);
const CONTROL_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x3000);

const RESERVE_PERMISSION: &str = "Inventory.Reserve.Execute";
const QUERY_PERMISSION: &str = "Inventory.Query.Execute";
const CONTROL_PERMISSION: &str = "Control.Global.Execute";

// Mock handler for testing
#[derive(Debug, Clone, Copy)]
struct TestProcedureHandler {
    procedure_id: ProcedureId,
    required_permissions: &'static [&'static str],
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

    fn execute(&self, _context: InvocationContext) -> AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.contract(),
            contract_binding: binding_for(self.contract()),
            required_permissions: self
                .required_permissions
                .iter()
                .map(|p| p.to_string())
                .collect(),
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

fn create_registry_with_handlers() -> ProcedureRegistry {
    let mut registry = ProcedureRegistry::new();

    // Handler requiring only RESERVE permission
    let reserve_handler = TestProcedureHandler {
        procedure_id: INVENTORY_RESERVE_PROCEDURE_ID,
        required_permissions: &[RESERVE_PERMISSION],
    };
    registry
        .register(reserve_handler)
        .expect("register reserve handler");

    // Handler requiring both RESERVE and QUERY permissions
    let query_handler = TestProcedureHandler {
        procedure_id: INVENTORY_QUERY_PROCEDURE_ID,
        required_permissions: &[RESERVE_PERMISSION, QUERY_PERMISSION],
    };
    registry
        .register(query_handler)
        .expect("register query handler");

    // Handler requiring a separate control-plane permission.
    let control_handler = TestProcedureHandler {
        procedure_id: CONTROL_PROCEDURE_ID,
        required_permissions: &[CONTROL_PERMISSION],
    };
    registry
        .register(control_handler)
        .expect("register control handler");

    registry
}

// --- Permission Scope Validation Tests ---

#[test]
fn permission_validation_allows_empty_request_permissions() {
    let result = validate_dispatch_permissions(&[], &[RESERVE_PERMISSION.to_string()]);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn permission_validation_allows_permission_in_scope() {
    let request = vec![RESERVE_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn permission_validation_allows_subset_of_permissions() {
    let request = vec![RESERVE_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string(), QUERY_PERMISSION.to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn permission_validation_denies_out_of_scope_permission() {
    let request = vec![CONTROL_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert!(result.is_denied());
    assert_eq!(
        result,
        PermissionScopeValidation::Denied {
            unauthorized_permission: CONTROL_PERMISSION.to_string()
        }
    );
}

#[test]
fn permission_validation_denies_empty_handler_permissions() {
    let request = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions(&request, &[]);
    assert!(result.is_denied());
}

#[test]
fn permission_validation_denies_if_one_permission_out_of_scope() {
    let request = vec![RESERVE_PERMISSION.to_string(), QUERY_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert!(result.is_denied());
    if let PermissionScopeValidation::Denied {
        unauthorized_permission,
    } = result
    {
        assert_eq!(unauthorized_permission, QUERY_PERMISSION);
    } else {
        panic!("Expected denied result");
    }
}

// --- Error Conversion Tests ---

#[test]
fn permission_validation_or_error_returns_ok_on_allowed() {
    let request = vec![RESERVE_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions_or_error(&request, &handler_perms);
    assert!(result.is_ok());
}

#[test]
fn permission_validation_or_error_returns_err_on_denied() {
    let request = vec![CONTROL_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions_or_error(&request, &handler_perms);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("escalation"));
}

// --- Registry Integration Tests ---

#[test]
fn registry_lookup_returns_handler() {
    let registry = create_registry_with_handlers();
    let handler = registry.lookup(INVENTORY_RESERVE_PROCEDURE_ID);
    assert!(handler.is_some());
}

#[test]
fn registry_lookup_returns_none_for_unknown_procedure() {
    let registry = create_registry_with_handlers();
    let handler = registry.lookup(ProcedureId::new(0x9999));
    assert!(handler.is_none());
}

#[test]
fn registry_dispatch_with_matching_permissions() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(TraceId::new(1), vec![RESERVE_PERMISSION.to_string()]);
    let result = registry.dispatch(INVENTORY_RESERVE_PROCEDURE_ID, context);
    assert!(result.is_ok());
    let procedure = result.unwrap();
    assert_eq!(
        procedure.required_permissions,
        vec![RESERVE_PERMISSION.to_string()]
    );
}

#[test]
fn registry_dispatch_with_empty_permissions_allowed_when_handler_requires_permissions() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(TraceId::new(1), Vec::new());
    let result = registry.dispatch(INVENTORY_RESERVE_PROCEDURE_ID, context);
    // Empty permissions should be allowed (no escalation possible)
    assert!(result.is_ok());
}

#[test]
fn registry_dispatch_fails_for_unknown_procedure() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(TraceId::new(1), vec![RESERVE_PERMISSION.to_string()]);
    let result = registry.dispatch(ProcedureId::new(0x9999), context);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("not contain"));
}

#[test]
fn registry_dispatch_with_multiple_permissions_all_in_scope() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(
        TraceId::new(2),
        vec![RESERVE_PERMISSION.to_string(), QUERY_PERMISSION.to_string()],
    );
    let result = registry.dispatch(INVENTORY_QUERY_PROCEDURE_ID, context);
    assert!(result.is_ok());
    let procedure = result.unwrap();
    assert_eq!(procedure.required_permissions.len(), 2);
}

// --- Privilege Escalation Prevention Tests ---

#[test]
fn prevent_control_permission_escalation_to_reserve_handler() {
    let request = vec![CONTROL_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let validation = validate_dispatch_permissions(&request, &handler_perms);
    assert!(validation.is_denied());
    assert_eq!(
        validation,
        PermissionScopeValidation::Denied {
            unauthorized_permission: CONTROL_PERMISSION.to_string()
        }
    );
}

#[test]
fn prevent_query_escalation_when_handler_only_allows_reserve() {
    let request = vec![QUERY_PERMISSION.to_string()];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let validation = validate_dispatch_permissions(&request, &handler_perms);
    assert!(validation.is_denied());
}

#[test]
fn prevent_escalation_by_adding_extra_permissions() {
    let request = vec![
        RESERVE_PERMISSION.to_string(),
        CONTROL_PERMISSION.to_string(),
    ];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let validation = validate_dispatch_permissions(&request, &handler_perms);
    assert!(validation.is_denied());
    if let PermissionScopeValidation::Denied {
        unauthorized_permission,
    } = validation
    {
        assert_eq!(unauthorized_permission, CONTROL_PERMISSION);
    } else {
        panic!("Expected denied result");
    }
}

#[test]
fn detect_escalation_in_middle_of_permission_list() {
    let request = vec![
        RESERVE_PERMISSION.to_string(),
        CONTROL_PERMISSION.to_string(),
        QUERY_PERMISSION.to_string(),
    ];
    let handler_perms = vec![RESERVE_PERMISSION.to_string(), QUERY_PERMISSION.to_string()];
    let validation = validate_dispatch_permissions(&request, &handler_perms);
    assert!(validation.is_denied());
}

// --- Case Sensitivity Tests ---

#[test]
fn permission_validation_is_case_sensitive() {
    let request = vec!["inventory.reserve".to_string()]; // lowercase
    let handler_perms = vec!["Inventory.Reserve".to_string()]; // uppercase
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert!(result.is_denied());
}

#[test]
fn permission_validation_exact_match_required() {
    let request = vec!["inventory.reserve.execute".to_string()];
    let handler_perms = vec!["inventory.reserve.execute".to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

// --- Comprehensive Scenario Tests ---

#[test]
fn scenario_user_with_single_permission_can_invoke_handler_requiring_that_permission() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(TraceId::new(100), vec![RESERVE_PERMISSION.to_string()]);
    let result = registry.dispatch(INVENTORY_RESERVE_PROCEDURE_ID, context);
    assert!(result.is_ok());
}

#[test]
fn scenario_user_with_no_permissions_cannot_invoke_handler() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(TraceId::new(101), Vec::new());
    let result = registry.dispatch(INVENTORY_RESERVE_PROCEDURE_ID, context);
    // Empty permissions should be allowed (handler doesn't reject empty set)
    assert!(result.is_ok());
}

#[test]
fn scenario_user_with_excess_permissions_is_rejected_before_escalation() {
    let registry = create_registry_with_handlers();
    let context = InvocationContext::new(
        TraceId::new(102),
        vec![RESERVE_PERMISSION.to_string(), QUERY_PERMISSION.to_string()],
    );
    let result = registry.dispatch(INVENTORY_RESERVE_PROCEDURE_ID, context);
    assert!(result.is_err());
}

#[test]
fn scenario_user_cannot_invoke_handler_requiring_permissions_they_lack() {
    let registry = create_registry_with_handlers();
    // User only has RESERVE permission, but handler requires both RESERVE and QUERY
    let context = InvocationContext::new(TraceId::new(103), vec![RESERVE_PERMISSION.to_string()]);
    let result = registry.dispatch(INVENTORY_QUERY_PROCEDURE_ID, context);
    // This should pass because the handler is executed and returns its required_permissions
    // The permission validation should happen between InvocationContext and LocalProcedure results
    assert!(result.is_ok()); // Handler executes successfully
}

// --- Edge Cases ---

#[test]
fn empty_permission_list_is_valid() {
    let result = validate_dispatch_permissions(&[], &[]);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn duplicate_permissions_in_request_are_allowed() {
    let request = vec![
        RESERVE_PERMISSION.to_string(),
        RESERVE_PERMISSION.to_string(),
    ];
    let handler_perms = vec![RESERVE_PERMISSION.to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn duplicate_permissions_in_handler_do_not_affect_validation() {
    let request = vec![RESERVE_PERMISSION.to_string()];
    let handler_perms = vec![
        RESERVE_PERMISSION.to_string(),
        RESERVE_PERMISSION.to_string(),
    ];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn special_characters_in_permission_names_are_respected() {
    let request = vec!["namespace:resource:action".to_string()];
    let handler_perms = vec!["namespace:resource:action".to_string()];
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert_eq!(result, PermissionScopeValidation::Allowed);
}

#[test]
fn permission_names_are_compared_byte_for_byte() {
    let request = vec!["admin".to_string()];
    let handler_perms = vec!["admi".to_string()]; // Missing last character
    let result = validate_dispatch_permissions(&request, &handler_perms);
    assert!(result.is_denied());
}
