use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_core::InvocationId;
use andromeda_exec::{
    CompletionStatus, InventoryReserveStockExecutor, InventoryStock, InvocationContext,
    InvocationRequest, LocalVerticalRuntime, ReserveStockCommand, SurfacePlaneAuthorizer,
};
use andromeda_observe::{
    AuthorizationDenialReason, AuthorizationOutcome, CertificateIdentity, Permission,
    PrincipalBinding, PrincipalRegistry, SecurityAuditOutcome, SurfaceScope, TraceId,
    UserPrincipal, UserPrincipalKind,
};
use andromeda_quic::SurfacePlane;
use andromeda_storage::InMemoryWal;

struct ExpectedDenial {
    trace_id: TraceId,
    plane: SurfacePlane,
    fingerprint: &'static str,
    reason: AuthorizationDenialReason,
    audit_reason_fragment: &'static str,
}

fn registry(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
    let mut registry = PrincipalRegistry::new();
    for binding in bindings {
        registry.register(binding).unwrap();
    }
    registry
}

fn binding(
    fingerprint: &str,
    surface: SurfaceScope,
    principal_id: &str,
    permissions: Vec<Permission>,
) -> PrincipalBinding {
    PrincipalBinding::new(
        CertificateIdentity::new(fingerprint, format!("CN={fingerprint}"), surface).unwrap(),
        UserPrincipal::new(principal_id, UserPrincipalKind::Service).unwrap(),
        permissions,
    )
    .unwrap()
}

fn assert_procedure_dispatch_denied_without_local_runtime_entry(
    registry: PrincipalRegistry,
    expected: ExpectedDenial,
) {
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let denied = gate
        .authorize_procedure_dispatch(expected.trace_id, expected.plane, expected.fingerprint)
        .unwrap()
        .unwrap_err();

    assert!(denied.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = denied {
        assert_eq!(reason, expected.reason);
        assert_eq!(audit.trace_id, expected.trace_id);
        assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
        assert!(audit.has_identity_evidence());
        assert!(!audit.contains_sensitive_evidence());
        assert!(
            audit.reason.contains(expected.reason.label()),
            "audit reason `{}` should contain typed denial label `{}`",
            audit.reason,
            expected.reason.label()
        );
        assert!(
            audit.reason.contains(expected.audit_reason_fragment),
            "audit reason `{}` should contain expected fragment `{}`",
            audit.reason,
            expected.audit_reason_fragment
        );
    } else {
        panic!("procedure dispatch should have been denied");
    }

    assert!(
        runtime.wal().is_empty(),
        "authorization denial must not create a WAL record"
    );
    assert_eq!(
        runtime.transactions().live_count(),
        0,
        "authorization denial must not create a local transaction"
    );
}

#[test]
fn denied_surface_dispatch_never_enters_local_transaction_runtime() {
    assert_procedure_dispatch_denied_without_local_runtime_entry(
        registry(Vec::new()),
        ExpectedDenial {
            trace_id: TraceId::new(44),
            plane: SurfacePlane::Application,
            fingerprint: "fp-unknown",
            reason: AuthorizationDenialReason::UnknownCertificate,
            audit_reason_fragment: "execute_procedure",
        },
    );
}

#[test]
fn procedure_dispatch_requires_application_surface_before_execution() {
    assert_procedure_dispatch_denied_without_local_runtime_entry(
        registry(vec![binding(
            "fp-admin",
            SurfaceScope::Administration,
            "ops-admin",
            vec![Permission::ExecuteProcedure],
        )]),
        ExpectedDenial {
            trace_id: TraceId::new(45),
            plane: SurfacePlane::Administration,
            fingerprint: "fp-admin",
            reason: AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
            audit_reason_fragment: "requires_application_surface",
        },
    );
}

#[test]
fn procedure_dispatch_rejects_certificate_scope_mismatch_before_transaction() {
    assert_procedure_dispatch_denied_without_local_runtime_entry(
        registry(vec![binding(
            "fp-admin-on-app",
            SurfaceScope::Administration,
            "ops-admin",
            vec![Permission::ExecuteProcedure],
        )]),
        ExpectedDenial {
            trace_id: TraceId::new(47),
            plane: SurfacePlane::Application,
            fingerprint: "fp-admin-on-app",
            reason: AuthorizationDenialReason::SurfaceScopeMismatch,
            audit_reason_fragment: "requested_surface=Application",
        },
    );
}

#[test]
fn procedure_dispatch_rejects_principal_missing_execute_permission_before_transaction() {
    assert_procedure_dispatch_denied_without_local_runtime_entry(
        registry(vec![binding(
            "fp-app-readonly",
            SurfaceScope::Application,
            "svc-readonly",
            vec![Permission::ReadContract],
        )]),
        ExpectedDenial {
            trace_id: TraceId::new(48),
            plane: SurfacePlane::Application,
            fingerprint: "fp-app-readonly",
            reason: AuthorizationDenialReason::PrincipalMissingPermission,
            audit_reason_fragment: "ExecuteProcedure",
        },
    );
}

#[test]
fn procedure_dispatch_rejects_monitoring_surface_before_transaction() {
    assert_procedure_dispatch_denied_without_local_runtime_entry(
        registry(vec![binding(
            "fp-monitoring",
            SurfaceScope::MonitoringAgent,
            "obs-agent",
            vec![Permission::ExecuteProcedure],
        )]),
        ExpectedDenial {
            trace_id: TraceId::new(49),
            plane: SurfacePlane::Monitoring,
            fingerprint: "fp-monitoring",
            reason: AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
            audit_reason_fragment: "surface=MonitoringAgent",
        },
    );
}

#[test]
fn procedure_dispatch_rejects_hadr_surface_before_transaction() {
    assert_procedure_dispatch_denied_without_local_runtime_entry(
        registry(vec![binding(
            "fp-hadr",
            SurfaceScope::Cluster,
            "ha-agent",
            vec![Permission::ExecuteProcedure],
        )]),
        ExpectedDenial {
            trace_id: TraceId::new(50),
            plane: SurfacePlane::HighAvailability,
            fingerprint: "fp-hadr",
            reason: AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
            audit_reason_fragment: "requires_application_surface",
        },
    );
}

#[test]
fn allowed_application_surface_dispatch_can_execute_with_token() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 2,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )
    .unwrap();
    let procedure = effect.to_local_procedure(&contract).unwrap();
    let context = InvocationContext::new(TraceId::new(46), contract.required_permissions.clone());
    let registry = registry(vec![binding(
        "fp-app",
        SurfaceScope::Application,
        "svc-app",
        vec![Permission::ExecuteProcedure],
    )]);
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let token = gate
        .authorize_procedure_dispatch(context.trace_id, SurfacePlane::Application, "fp-app")
        .unwrap()
        .unwrap();
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_surface_authorized(
            InvocationRequest {
                invocation_id: InvocationId::new(46),
                procedure: contract.as_ref(),
                expected_contract_hash: contract.contract_hash,
                catalog_version: contract.object.catalog_version,
                structured_parameters: Vec::new(),
            },
            &procedure,
            &context,
            &token,
        )
        .unwrap();

    assert_eq!(token.plane(), SurfacePlane::Application);
    assert_eq!(token.audit().outcome, SecurityAuditOutcome::Allowed);
    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert!(!runtime.wal().is_empty());
}
