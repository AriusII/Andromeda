use super::support::*;

#[test]
fn local_vertical_runtime_rejects_missing_permission_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_internal_authorized(
            inventory_request_with_id(&contract, 701),
            &procedure,
            &InvocationContext::new(TraceId::new(7001), Vec::new()),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(runtime.wal().is_empty());
}

#[test]
fn local_vertical_runtime_rejects_permissioned_execute_without_context_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_internal(inventory_request(&contract), &procedure, TraceId::new(7005))
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("authorization context"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn local_vertical_runtime_rejects_permissioned_rollback_without_context_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let err = runtime
        .rollback_business_validation_failure_after_begin(
            inventory_request(&contract),
            &procedure,
            TraceId::new(7006),
            "insufficient inventory stock for reservation",
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("authorization context"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn surface_dispatch_denial_stops_before_transaction_creation() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let context = inventory_context(&contract, 7002);
    let registry = principal_registry(Vec::new());
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let denied = gate
        .authorize_procedure_dispatch(
            context.trace_id,
            SurfacePlane::Application,
            "fp-not-registered",
        )
        .unwrap()
        .unwrap_err();

    assert!(denied.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = denied {
        assert_eq!(reason, AuthorizationDenialReason::UnknownCertificate);
        assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
        assert!(audit.has_identity_evidence());
    }
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn surface_authorized_dispatch_token_executes_through_local_runtime() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = inventory_context(&contract, 7003);
    let registry = principal_registry(vec![principal_binding(
        "fp-app-dispatch",
        SurfaceScope::Application,
        "svc-dispatch",
        vec![Permission::ExecuteProcedure],
    )]);
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let token = gate
        .authorize_procedure_dispatch(
            context.trace_id,
            SurfacePlane::Application,
            "fp-app-dispatch",
        )
        .unwrap()
        .unwrap();
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_surface_authorized(inventory_request(&contract), &procedure, &context, &token)
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(token.audit().outcome, SecurityAuditOutcome::Allowed);
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn administration_surface_cannot_present_procedure_dispatch_as_external_runtime_path() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let registry = principal_registry(vec![principal_binding(
        "fp-admin-with-exec",
        SurfaceScope::Administration,
        "ops-dispatch",
        vec![Permission::ExecuteProcedure],
    )]);
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let denied = gate
        .authorize_procedure_dispatch(
            TraceId::new(7004),
            SurfacePlane::Administration,
            "fp-admin-with-exec",
        )
        .unwrap()
        .unwrap_err();

    assert!(denied.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = denied {
        assert_eq!(
            reason,
            AuthorizationDenialReason::SurfaceDoesNotPermitPermission
        );
        assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
        assert!(audit.reason.contains("requires_application_surface"));
    }
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
    assert!(contract.required_permissions.iter().all(|p| !p.is_empty()));
}
