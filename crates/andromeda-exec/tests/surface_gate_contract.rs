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
        runtime.transactions().live_count().unwrap(),
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
    assert_eq!(outcome.completion.status(), CompletionStatus::Committed);
    assert!(!runtime.wal().is_empty());
}

// D3: Certificate Identity Binding Contracts

/// Contract test: Verify that a Connection with bound certificate identity
/// can provide the fingerprint to surface_gate for authorization.
#[test]
fn d3_certificate_identity_binding_to_surface_gate() {
    use andromeda_quic::Connection;

    // Create a connection bound to Application plane.
    let mut conn = Connection::new(SurfacePlane::Application);

    // Bind a certificate identity matching the plane scope.
    let cert_identity = CertificateIdentity::new(
        "a".repeat(64), // SHA256 fingerprint
        "test-service",
        SurfaceScope::Application,
    )
    .unwrap();

    conn.set_certificate_identity(cert_identity.clone())
        .unwrap();

    // Create a registry with a binding for this fingerprint.
    let registry = registry(vec![binding(
        &"a".repeat(64),
        SurfaceScope::Application,
        "svc-app",
        vec![Permission::ExecuteProcedure],
    )]);

    let gate = SurfacePlaneAuthorizer::new(&registry);

    // Retrieve the fingerprint from the connection's certificate identity.
    let fp = conn
        .certificate_identity()
        .map(|ci| ci.fingerprint.as_str())
        .expect("connection should have certificate identity");

    // Authorize dispatch using the bound identity.
    let outcome = gate
        .authorize_dispatch(
            TraceId::new(100),
            SurfacePlane::Application,
            fp,
            andromeda_observe::SurfaceAction::ExecuteProcedure,
        )
        .unwrap();

    assert!(outcome.is_allowed());
}

/// Contract test: Verify that certificate identity mismatch at session construction
/// prevents dispatch authorization (surface plane scope check).
#[test]
fn d3_certificate_scope_mismatch_prevents_dispatch() {
    use andromeda_quic::Connection;

    // Create a connection bound to Application plane.
    let mut conn = Connection::new(SurfacePlane::Application);

    // Try to bind an Administration certificate (wrong scope).
    let admin_cert = CertificateIdentity::new(
        "b".repeat(64),
        "admin-service",
        SurfaceScope::Administration, // Mismatch!
    )
    .unwrap();

    // Binding should fail due to scope mismatch.
    let err = conn.set_certificate_identity(admin_cert);
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().kind(),
        andromeda_core::AndromedaErrorKind::Protocol
    );

    // Connection should have no identity bound.
    assert!(conn.certificate_identity().is_none());
}

/// Contract test: Verify that once a certificate identity is bound,
/// it cannot be replaced.
#[test]
fn d3_certificate_identity_immutability() {
    use andromeda_quic::Connection;

    let mut conn = Connection::new(SurfacePlane::Administration);

    let cert1 =
        CertificateIdentity::new("c".repeat(64), "admin-1", SurfaceScope::Administration).unwrap();
    let cert2 =
        CertificateIdentity::new("d".repeat(64), "admin-2", SurfaceScope::Administration).unwrap();

    // First binding succeeds.
    conn.set_certificate_identity(cert1.clone()).unwrap();
    assert_eq!(
        conn.certificate_identity().unwrap().fingerprint,
        "c".repeat(64)
    );

    // Second binding fails.
    let err = conn.set_certificate_identity(cert2);
    assert!(err.is_err());

    // First identity is preserved.
    assert_eq!(
        conn.certificate_identity().unwrap().fingerprint,
        "c".repeat(64)
    );
}

/// Contract test: Verify that all four surface planes enforce their
/// required certificate scopes.
#[test]
fn d3_all_planes_enforce_certificate_scope_policy() {
    use andromeda_quic::Connection;

    let test_cases = vec![
        (
            SurfacePlane::Application,
            SurfaceScope::Application,
            true, // should succeed
        ),
        (
            SurfacePlane::Application,
            SurfaceScope::Administration,
            false, // should fail
        ),
        (
            SurfacePlane::Administration,
            SurfaceScope::Administration,
            true,
        ),
        (
            SurfacePlane::Administration,
            SurfaceScope::Application,
            false,
        ),
        (SurfacePlane::HighAvailability, SurfaceScope::Cluster, true),
        (
            SurfacePlane::HighAvailability,
            SurfaceScope::Application,
            false,
        ),
        (
            SurfacePlane::Monitoring,
            SurfaceScope::MonitoringAgent,
            true,
        ),
        (SurfacePlane::Monitoring, SurfaceScope::Application, false),
    ];

    for (plane, scope, should_succeed) in test_cases {
        let mut conn = Connection::new(plane);
        let identity = CertificateIdentity::new("e".repeat(64), "test", scope).unwrap();
        let result = conn.set_certificate_identity(identity);

        if should_succeed {
            assert!(
                result.is_ok(),
                "plane {:?} with scope {:?} should succeed",
                plane,
                scope
            );
        } else {
            assert!(
                result.is_err(),
                "plane {:?} with scope {:?} should fail",
                plane,
                scope
            );
        }
    }
}
