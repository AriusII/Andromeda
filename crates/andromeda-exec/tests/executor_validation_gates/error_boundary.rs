use super::support::*;

#[derive(Clone)]
struct MockRoutingResolver {
    primary: ProcedureResolveResponse,
    secondary: ProcedureResolveResponse,
}

impl ProcedureResolver for MockRoutingResolver {
    fn resolve_procedure(
        &self,
        request: ProcedureResolveRequest,
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        let response = match request.target {
            andromeda_procedure_runtime::procedure_resolver::ProcedureResolveTarget::ProcedureId(id)
                if id == self.primary.procedure_id =>
            {
                self.primary.clone()
            },
            andromeda_procedure_runtime::procedure_resolver::ProcedureResolveTarget::ProcedureId(id)
                if id == self.secondary.procedure_id =>
            {
                self.secondary.clone()
            },
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
fn gate_exec_02_srpl_adapter_dispatches_through_catalog_backed_dispatcher_path() {
    let resolver = Arc::new(MockValidResolver::new());
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::with_resolver_dispatcher(dispatcher);

    let procedure = adapter
        .dispatch_procedure(valid_dispatch_request(TraceId::new(2)))
        .expect("catalog-backed dispatcher path should execute after SRPL resolution");

    assert_eq!(procedure.contract, contract_ref());
    assert_eq!(procedure.contract_binding, contract_binding());
    assert_eq!(procedure.rows_affected, 1);
    assert!(!procedure.mutation_payload.is_empty());
    assert_eq!(
        procedure.required_permissions,
        vec!["Test.Procedure.Execute".to_string()]
    );
}

#[test]
fn gate_exec_02_srpl_adapter_dispatches_two_cataloged_procedures_without_special_handlers() {
    let primary = valid_response();
    let secondary_contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(2),
        contract_hash: ContractHash::test_vector(9),
        catalog_version: CatalogVersion::new(1),
    };
    let secondary_binding = ProcedureContractBinding {
        procedure_id: secondary_contract.procedure_id,
        catalog_version: secondary_contract.catalog_version,
        contract_hash: secondary_contract.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([9; PolicyVersion::LEN]),
    };
    let mut secondary = valid_response();
    secondary.procedure_id = secondary_contract.procedure_id;
    secondary.contract_hash = secondary_contract.contract_hash;
    secondary.catalog_version = secondary_contract.catalog_version;
    secondary.contract_ref = secondary_contract;
    secondary.manifest.contract_ref = secondary_contract;
    secondary.name = QualifiedName::parse("Test.SecondProcedure").expect("valid qualified name");
    secondary.plan.procedure_name = secondary.name.clone();
    secondary.plan.evidence.procedure_object.name = secondary.name.clone();
    secondary.plan.evidence.procedure_contract = secondary_contract;
    secondary.manifest.required_permissions = vec!["Test.SecondProcedure.Execute".to_string()];
    secondary
        .validate()
        .expect("secondary resolver response must remain valid");

    let resolver = Arc::new(MockRoutingResolver { primary, secondary });
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::with_resolver_dispatcher(dispatcher);

    let request1 = valid_dispatch_request(TraceId::new(12));
    let mut request2 = valid_dispatch_request(TraceId::new(13));
    request2.procedure = secondary_contract;
    request2.procedure_binding = Some(secondary_binding);
    request2.context = invocation_context(TraceId::new(13));
    request2.pre_transaction = PreTransactionDispatchEvidence {
        admission_trace: decision_trace(TraceId::new(13), CriticalDecisionKind::ResourceGovernance),
        contract_trace: decision_trace(TraceId::new(13), CriticalDecisionKind::ContractValidation),
        authorization_trace: None,
    };

    let procedure1 = adapter
        .dispatch_procedure(request1)
        .expect("first cataloged procedure should dispatch through SRPL adapter");
    let procedure2 = adapter
        .dispatch_procedure(request2)
        .expect("second cataloged procedure should dispatch through SRPL adapter");

    assert_eq!(procedure1.contract, contract_ref());
    assert_eq!(procedure2.contract, secondary_contract);
    assert_eq!(
        procedure2.required_permissions,
        vec!["Test.SecondProcedure.Execute".to_string()]
    );
    assert_ne!(
        procedure1.contract.procedure_id,
        procedure2.contract.procedure_id
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
        payload: ProcedureDispatchPayload::empty(),
        principal: PrincipalId::new(1),
        deadline: ProcedureDeadline::new(std::time::Duration::from_secs(30)),
        idempotency_key: None,
    };

    let result = adapter.dispatch_procedure(request);
    assert!(
        result.is_err(),
        "invalid trace correlation must be rejected at the request boundary"
    );
}

#[test]
fn gate_exec_02_unknown_cataloged_procedure_maps_to_catalog_error() {
    let primary = valid_response();
    let mut secondary = valid_response();
    let secondary_contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(2),
        contract_hash: ContractHash::test_vector(9),
        catalog_version: CatalogVersion::new(1),
    };
    secondary.procedure_id = secondary_contract.procedure_id;
    secondary.contract_hash = secondary_contract.contract_hash;
    secondary.catalog_version = secondary_contract.catalog_version;
    secondary.contract_ref = secondary_contract;
    secondary.manifest.contract_ref = secondary_contract;
    secondary.name = QualifiedName::parse("Test.SecondProcedure").expect("valid qualified name");
    secondary.plan.procedure_name = secondary.name.clone();
    secondary.plan.evidence.procedure_object.name = secondary.name.clone();
    secondary.plan.evidence.procedure_contract = secondary_contract;
    secondary
        .validate()
        .expect("secondary response must be valid");

    let resolver = Arc::new(MockRoutingResolver { primary, secondary });
    let dispatcher = srpl_dispatcher(resolver);
    let adapter = SrplDispatcherAdapter::with_resolver_dispatcher(dispatcher);

    let unknown_contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(99),
        contract_hash: ContractHash::test_vector(0x99),
        catalog_version: CatalogVersion::new(1),
    };
    let unknown_binding = ProcedureContractBinding {
        procedure_id: unknown_contract.procedure_id,
        catalog_version: unknown_contract.catalog_version,
        contract_hash: unknown_contract.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([0x99; PolicyVersion::LEN]),
    };
    let request = ProcedureDispatchRequest {
        invocation_id: InvocationId::new(404),
        procedure: unknown_contract,
        procedure_binding: Some(unknown_binding),
        context: invocation_context(TraceId::new(14)),
        pre_transaction: PreTransactionDispatchEvidence {
            admission_trace: decision_trace(
                TraceId::new(14),
                CriticalDecisionKind::ResourceGovernance,
            ),
            contract_trace: decision_trace(
                TraceId::new(14),
                CriticalDecisionKind::ContractValidation,
            ),
            authorization_trace: None,
        },
        payload: ProcedureDispatchPayload::empty(),
        principal: PrincipalId::new(1),
        deadline: ProcedureDeadline::new(std::time::Duration::from_secs(30)),
        idempotency_key: None,
    };

    let err = adapter
        .dispatch_procedure(request)
        .expect_err("unknown procedure id must fail-closed");
    assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
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
