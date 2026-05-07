use super::support::*;

#[test]
fn gate_exec_02_error_boundary_pre_transaction() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);
    let request = valid_dispatch_request(TraceId::new(1));

    let err = adapter.dispatch_procedure(request).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
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
        MockLocalDispatcher::new(local_procedure()),
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

    assert_eq!(err.kind(), AndromedaErrorKind::Execution);
    assert!(err.message().contains("invocation id"));
}

#[test]
fn gate_exec_06_error_handling_deterministic() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::new(dispatcher);
    let request = valid_dispatch_request(TraceId::new(1));

    let result1 = adapter.dispatch_procedure(request.clone());
    let result2 = adapter.dispatch_procedure(request);

    assert!(result1.is_err(), "First attempt should fail");
    assert!(result2.is_err(), "Second attempt should fail");
    assert_eq!(
        result1.unwrap_err().message(),
        result2.unwrap_err().message()
    );
}
