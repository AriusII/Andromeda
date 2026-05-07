use super::support::*;

#[test]
fn procedure_handler_exposes_metadata_and_existing_local_result_shape() {
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
        .expect("fake handler result should satisfy existing LocalProcedure shape");

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
    let dispatcher = RemoteProcedureDispatcherUnavailable::unsupported();
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
