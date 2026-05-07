use andromeda_core::{
    CertificateIdentityStatus, Permission, PermissionSet, PrincipalAuthorizationDenialReason,
    PrincipalRegistry, PrincipalRole, PrincipalStatus, ProcedureId, SurfaceScope,
};

use crate::principal_fixtures::{
    principal_binding, principal_binding_with_direct_permissions, seeded_fingerprint,
    test_fingerprint,
};

#[test]
fn test_certificate_surface_scope_mismatch_denies_before_permission_match() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-admin",
        SurfaceScope::Administration,
        808,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fingerprint,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::SurfaceScopeMismatch)
    );
    assert_eq!(decision.evidence.surface_scope, SurfaceScope::Application);
    assert_eq!(decision.evidence.reason, "surface_scope_mismatch");
}

#[test]
fn test_surface_policy_denial_is_observable_before_role_permission_match() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-application-superadmin",
        SurfaceScope::Application,
        809,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
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
    assert_eq!(
        decision.evidence.certificate_status,
        Some(CertificateIdentityStatus::Active)
    );
    assert_eq!(
        decision.evidence.principal_status,
        Some(PrincipalStatus::Active)
    );
    assert_eq!(
        decision.evidence.certificate_surface_scope,
        Some(SurfaceScope::Application)
    );
    assert!(decision.evidence.surface_policy_evaluated);
    assert!(!decision.evidence.surface_policy_allowed);
    assert!(!decision.evidence.role_permission_evaluated);
    assert!(!decision.evidence.direct_permission_evaluated);
    assert_eq!(
        decision.evidence.reason,
        "surface_does_not_permit_permission"
    );
}

#[test]
fn test_missing_permission_denial_records_role_and_direct_checks() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-admin-user",
        SurfaceScope::Administration,
        810,
        PrincipalRole::User,
        PrincipalStatus::Active,
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Administration,
        &fingerprint,
        &Permission::AdminShutdown,
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::PrincipalMissingPermission)
    );
    assert!(decision.evidence.surface_policy_evaluated);
    assert!(decision.evidence.surface_policy_allowed);
    assert!(decision.evidence.role_permission_evaluated);
    assert!(!decision.evidence.role_permission_granted);
    assert!(decision.evidence.direct_permission_evaluated);
    assert!(!decision.evidence.direct_permission_granted);
    assert_eq!(decision.evidence.reason, "principal_missing_permission");
}

#[test]
fn test_direct_permission_absence_is_a_denial_not_an_implicit_grant() {
    let fingerprint = seeded_fingerprint('9');
    let binding = principal_binding_with_direct_permissions(
        fingerprint.clone(),
        "CN=svc-admin-direct-deny",
        SurfaceScope::Administration,
        819,
        PrincipalRole::User,
        PrincipalStatus::Active,
        PermissionSet::new().with_permission(Permission::AuditRead),
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Administration,
        &fingerprint,
        &Permission::AdminShutdown,
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::PrincipalMissingPermission)
    );
    assert!(decision.evidence.surface_policy_allowed);
    assert!(decision.evidence.role_permission_evaluated);
    assert!(!decision.evidence.role_permission_granted);
    assert!(decision.evidence.direct_permission_evaluated);
    assert!(!decision.evidence.direct_permission_granted);
}
