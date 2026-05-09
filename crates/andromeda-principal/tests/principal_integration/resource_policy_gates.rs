use andromeda_principal::{
    CertificateIdentity, Permission, PermissionSet, PrincipalAuthorizationDenialReason,
    PrincipalBinding, PrincipalId, PrincipalRegistry, PrincipalRole, PrincipalStatus, SessionToken,
    SurfaceScope,
};
use andromeda_types::ProcedureId;

use crate::principal_fixtures::{principal_binding, seeded_fingerprint};

#[test]
fn test_application_administration_and_cluster_surfaces_are_separate() {
    let mut registry = PrincipalRegistry::new();
    for (fingerprint, surface_scope, principal_id) in [
        (seeded_fingerprint('c'), SurfaceScope::Application, 815),
        (seeded_fingerprint('d'), SurfaceScope::Administration, 816),
        (seeded_fingerprint('e'), SurfaceScope::Cluster, 817),
    ] {
        registry
            .register(principal_binding(
                fingerprint,
                "CN=svc-surface",
                surface_scope,
                principal_id,
                PrincipalRole::SuperAdmin,
                PrincipalStatus::Active,
            ))
            .expect("binding registered");
    }

    for (presented_fingerprint, requested_scope, required_permission) in [
        (
            seeded_fingerprint('c'),
            SurfaceScope::Administration,
            Permission::AdminShutdown,
        ),
        (
            seeded_fingerprint('d'),
            SurfaceScope::Cluster,
            Permission::AdminShutdown,
        ),
        (
            seeded_fingerprint('e'),
            SurfaceScope::Application,
            Permission::ExecuteProcedure(ProcedureId::new(42)),
        ),
    ] {
        let decision = registry.authorize(
            requested_scope,
            &presented_fingerprint,
            &required_permission,
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::SurfaceScopeMismatch)
        );
        assert_eq!(decision.evidence.surface_scope, requested_scope);
        assert_eq!(decision.evidence.reason, "surface_scope_mismatch");
        assert!(!decision.evidence.role_permission_evaluated);
        assert!(!decision.evidence.direct_permission_evaluated);
    }
}

#[test]
fn test_cluster_surface_rejects_admin_permissions_and_allows_cluster_permissions() {
    let fingerprint = seeded_fingerprint('e');
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-cluster-policy",
        SurfaceScope::Cluster,
        821,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
    );

    assert!(
        !binding.role_grants(&Permission::AdminShutdown),
        "cluster certificates must not carry administration permissions"
    );
    assert!(
        binding.role_grants(&Permission::ClusterPromote),
        "cluster certificates can carry explicit cluster permissions"
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let admin_decision = registry.authorize(
        SurfaceScope::Cluster,
        &fingerprint,
        &Permission::AdminShutdown,
    );
    assert!(admin_decision.is_denied());
    assert_eq!(
        admin_decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission)
    );
    assert!(!admin_decision.evidence.role_permission_evaluated);

    let cluster_decision = registry.authorize(
        SurfaceScope::Cluster,
        &fingerprint,
        &Permission::ClusterPromote,
    );
    assert!(cluster_decision.is_allowed());
    assert!(cluster_decision.evidence.surface_policy_allowed);
    assert!(cluster_decision.evidence.role_permission_granted);
}

#[test]
fn test_direct_permission_binding_rejects_permission_outside_certificate_surface() {
    let fingerprint = seeded_fingerprint('8');
    let certificate = CertificateIdentity::new(
        fingerprint,
        "CN=svc-application-direct-admin",
        SurfaceScope::Application,
    )
    .expect("valid certificate identity");
    let principal = andromeda_principal::Principal::new(
        PrincipalId::new(821),
        PrincipalRole::User,
        SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
        certificate.fingerprint().clone(),
    )
    .expect("active principal");

    let binding = PrincipalBinding::new_with_direct_permissions(
        certificate,
        principal,
        PermissionSet::new().with_permission(Permission::AdminShutdown),
    );

    assert!(
        binding.is_err(),
        "Application certificate direct grants must not store Admin permissions"
    );
}

#[test]
fn test_role_permissions_do_not_escape_certificate_surface_scope() {
    let fingerprint = seeded_fingerprint('d');
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-application-superadmin",
        SurfaceScope::Application,
        820,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
    );

    assert!(
        !binding.role_grants(&Permission::AdminShutdown),
        "role permissions must not bypass certificate surface scope"
    );
    assert!(
        !binding.grants(&Permission::AdminShutdown),
        "aggregate grant helpers must remain surface-scoped"
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fingerprint,
        &Permission::AdminShutdown,
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission)
    );
    assert!(!decision.evidence.role_permission_evaluated);
    assert!(!decision.evidence.direct_permission_evaluated);
}
