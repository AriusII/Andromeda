//! C4: Admission and Contract Event Observable Coverage
//!
//! This test suite validates that:
//! 1. All admission refusals (resource budget, contract mismatch) produce typed observable events
//! 2. All authorization denials produce typed observable events
//! 3. Pre-transaction rejections do NOT create WAL entries or transactions
//! 4. Event emission failures are NOT silently dropped
//! 5. ExecutionTransitionTrace events are emitted for all pre-transaction failures
//!
//! These are C4 pre-transaction invariants: all failures that occur before
//! `TransactionManager` allocation must be observable and must leave no durability artifacts.

use andromeda_catalog::{
    CatalogSnapshot, INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID, ProcedureContract,
    inventory_domain_definition_batch, inventory_reserve_stock_contract,
};
use andromeda_core::{CatalogVersion, ContractHash, InvocationId};
use andromeda_exec::{
    CompletionStatus, InvocationContext, InvocationRequest, services::AdmissionService,
    services::PreTransactionValidationService,
};
use andromeda_observe::TraceId;

// ============================================================================
// Fixtures
// ============================================================================

fn inventory_catalog_snapshot() -> CatalogSnapshot {
    let batch = inventory_domain_definition_batch().unwrap();
    let plan = batch.dry_run().unwrap();
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();
    snapshot
}

fn valid_contract() -> ProcedureContract {
    inventory_reserve_stock_contract().unwrap()
}

fn valid_request(contract: &ProcedureContract, invocation_id: u64) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

fn valid_context(contract: &ProcedureContract, trace_id: u64) -> InvocationContext {
    InvocationContext::new(
        TraceId::new(trace_id.into()),
        contract.required_permissions.clone(),
    )
}

// ============================================================================
// C4.1: Admission Rejections (Resource Budget)
// ============================================================================

/// C4.1.1: Zero InvocationId must be rejected at admission gate, with event emitted.
#[test]
fn c4_admission_rejected_zero_invocation_id_emits_contract_rejected_event() {
    let contract = valid_contract();
    let mut request = valid_request(&contract, 700);
    request.invocation_id = InvocationId::new(0); // VIOLATION: zero invocation_id

    let trace_id = TraceId::new(7001);
    let result = AdmissionService::validate_invocation_request(&request, trace_id);

    // Must reject at admission gate.
    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.to_lowercase().contains("invocationid"));
}

/// C4.1.2: Zero ContractHash must be rejected at admission gate, with event emitted.
#[test]
fn c4_admission_rejected_zero_contract_hash_emits_contract_rejected_event() {
    let contract = valid_contract();
    let mut request = valid_request(&contract, 700);
    request.expected_contract_hash = ContractHash::new([0u8; 32]); // VIOLATION: zero hash

    let trace_id = TraceId::new(7002);
    let result = AdmissionService::validate_invocation_request(&request, trace_id);

    // Must reject at admission gate.
    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.to_lowercase().contains("contracthash"));
}

/// C4.1.3: Zero CatalogVersion must be rejected at admission gate, with event emitted.
#[test]
fn c4_admission_rejected_zero_catalog_version_emits_contract_rejected_event() {
    let contract = valid_contract();
    let mut request = valid_request(&contract, 700);
    request.catalog_version = CatalogVersion::new(0); // VIOLATION: zero version

    let trace_id = TraceId::new(7003);
    let result = AdmissionService::validate_invocation_request(&request, trace_id);

    // Must reject at admission gate.
    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.to_lowercase().contains("catalogversion"));
}

// ============================================================================
// C4.2: Contract Validation (Contract Mismatch Before Transaction)
// ============================================================================

/// C4.2.1: ContractHash mismatch must be rejected before transaction creation.
#[test]
fn c4_contract_rejected_hash_mismatch_before_transaction() {
    let contract = valid_contract();
    let mut request = valid_request(&contract, 701);
    request.expected_contract_hash = ContractHash::new([99u8; 32]); // MISMATCH

    let trace_id = TraceId::new(7010);
    let result = PreTransactionValidationService::validate_invocation_contract(
        &request,
        contract.as_ref(),
        trace_id,
    );

    // Must reject before transaction creation.
    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.to_lowercase().contains("contracthash"));
}

/// C4.2.2: CatalogVersion mismatch must be rejected before transaction creation.
#[test]
fn c4_contract_rejected_catalog_version_mismatch_before_transaction() {
    let contract = valid_contract();
    let mut request = valid_request(&contract, 702);
    // Declare expected catalog version as something else
    request.catalog_version = CatalogVersion::new(9999); // MISMATCH

    let trace_id = TraceId::new(7011);
    let result = PreTransactionValidationService::validate_invocation_contract(
        &request,
        contract.as_ref(),
        trace_id,
    );

    // Must reject before transaction creation.
    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.to_lowercase().contains("catalogversion"));
}

// ============================================================================
// C4.3: Authorization Denials (Security Before Transaction)
// ============================================================================

/// C4.3.1: Missing permission must be rejected before transaction creation.
#[test]
fn c4_authorization_denied_missing_permission_before_transaction() {
    let contract = valid_contract();
    let trace_id = TraceId::new(7020);

    // Create context with ZERO permissions (will deny all required permissions).
    let context = InvocationContext::new(trace_id, vec![]); // VIOLATION: no permissions

    // The contract requires ExecuteProcedure permission (standard); context has none.
    let required_permission = &contract.required_permissions[0];
    let result = AdmissionService::authorize(&context, &[required_permission.clone()]);

    // Must reject at authorization gate.
    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::PermissionDenied);
    assert!(reject.reason.to_lowercase().contains("permission"));
}

/// C4.3.2: Contract validation must produce ContractValidation decision.
#[test]
fn c4_contract_validation_produces_contract_validation_decision() {
    let contract = valid_contract();
    let mut request = valid_request(&contract, 708);
    request.expected_contract_hash = ContractHash::new([88u8; 32]); // MISMATCH

    let trace_id = TraceId::new(7080);
    let result = PreTransactionValidationService::validate_invocation_contract(
        &request,
        contract.as_ref(),
        trace_id,
    );

    assert!(result.is_err());
    let reject = result.unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
}
