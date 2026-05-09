use super::*;

#[test]
fn test_gateway_authorized_route_allows_core_principal_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let (registry, principal_id) = registry_for_application_user();

    let authorized = gateway
        .bind_authorized_application_procedure_route(42, &frame, &manifest, &registry)
        .expect("core IAM should allow the route before dispatch");

    assert_eq!(authorized.route.invocation_id, InvocationId::new(42));
    assert_eq!(authorized.route.procedure_id, ProcedureId::new(42));
    assert_eq!(authorized.principal_id, principal_id);
    assert_common_authorization_evidence(
        &authorized.authorization_evidence,
        PrincipalAuthorizationOutcome::Allowed,
        "allowed",
        PrincipalAuthorizationEvaluationStage::Allowed,
        &registry,
    );
    assert_eq!(
        authorized.authorization_evidence.required_permission,
        Permission::ExecuteProcedure(ProcedureId::new(42))
    );
    assert_eq!(
        authorized.authorization_evidence.surface_scope,
        CoreSurfaceScope::Application
    );
    assert_eq!(
        authorized.authorization_evidence.certificate_surface_scope,
        Some(CoreSurfaceScope::Application)
    );
    assert_eq!(
        authorized.authorization_evidence.certificate_fingerprint,
        "a".repeat(64)
    );
    assert_eq!(
        authorized.authorization_evidence.certificate_subject,
        "app-service"
    );
    assert_eq!(
        authorized.authorization_evidence.certificate_status,
        Some(CertificateIdentityStatus::Active)
    );
    assert_eq!(
        authorized.authorization_evidence.principal_id,
        Some(principal_id)
    );
    assert_eq!(
        authorized.authorization_evidence.principal_status,
        Some(PrincipalStatus::Active)
    );
    assert!(authorized.authorization_evidence.surface_policy_evaluated);
    assert!(authorized.authorization_evidence.surface_policy_allowed);
    assert!(authorized.authorization_evidence.role_permission_evaluated);
    assert!(authorized.authorization_evidence.role_permission_granted);
    assert!(
        authorized
            .authorization_evidence
            .direct_permission_evaluated
    );
    assert!(!authorized.authorization_evidence.direct_permission_granted);
}

#[test]
fn test_gateway_authorized_route_rejects_unknown_certificate_before_dispatch() {
    assert_authorized_route_denial(
        PrincipalRegistry::new(),
        PrincipalAuthorizationDenialReason::UnknownCertificate,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_revoked_certificate_before_dispatch() {
    assert_application_user_certificate_denial(
        |registry| {
            registry
                .revoke_certificate(&"a".repeat(64))
                .expect("registered certificate can be revoked");
        },
        PrincipalAuthorizationDenialReason::CertificateRevoked,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_disabled_certificate_before_dispatch() {
    assert_application_user_certificate_denial(
        |registry| {
            registry
                .disable_certificate(&"a".repeat(64))
                .expect("registered certificate can be disabled");
        },
        PrincipalAuthorizationDenialReason::CertificateDisabled,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_disabled_principal_before_dispatch() {
    assert_binding_denial(
        CoreSurfaceScope::Application,
        PrincipalRole::User,
        PrincipalStatus::Disabled,
        PermissionSet::new(),
        PrincipalAuthorizationDenialReason::PrincipalDisabled,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_core_surface_scope_mismatch_before_dispatch() {
    assert_binding_denial(
        CoreSurfaceScope::Administration,
        PrincipalRole::User,
        PrincipalStatus::Active,
        PermissionSet::new(),
        PrincipalAuthorizationDenialReason::SurfaceScopeMismatch,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_missing_execute_permission_before_dispatch() {
    assert_binding_denial(
        CoreSurfaceScope::Application,
        PrincipalRole::Guest,
        PrincipalStatus::Active,
        PermissionSet::new(),
        PrincipalAuthorizationDenialReason::PrincipalMissingPermission,
    );
}

fn assert_application_user_certificate_denial(
    mutate_registry: impl FnOnce(&mut PrincipalRegistry),
    expected_reason: PrincipalAuthorizationDenialReason,
) {
    let (mut registry, _) = registry_for_application_user();
    mutate_registry(&mut registry);

    assert_authorized_route_denial(registry, expected_reason);
}

fn assert_binding_denial(
    certificate_scope: CoreSurfaceScope,
    role: PrincipalRole,
    status: PrincipalStatus,
    direct_permissions: PermissionSet,
    expected_reason: PrincipalAuthorizationDenialReason,
) {
    let (registry, _) = registry_with_binding(
        &"a".repeat(64),
        certificate_scope,
        role,
        status,
        direct_permissions,
    );

    assert_authorized_route_denial(registry, expected_reason);
}
