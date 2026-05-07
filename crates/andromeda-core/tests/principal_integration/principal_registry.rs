use andromeda_core::{
    Permission, PermissionSet, PrincipalBinding, PrincipalRegistry, PrincipalRole, PrincipalStatus,
    SurfaceScope,
};

use crate::principal_fixtures::{
    certificate_identity, principal_binding_with_direct_permissions, principal_for_certificate,
    seeded_fingerprint, test_fingerprint,
};

#[test]
fn test_direct_permission_grant_is_observable_when_role_does_not_grant() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding_with_direct_permissions(
        fingerprint.clone(),
        "CN=svc-admin-direct",
        SurfaceScope::Administration,
        811,
        PrincipalRole::User,
        PrincipalStatus::Active,
        PermissionSet::new().with_permission(Permission::AdminShutdown),
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Administration,
        &fingerprint,
        &Permission::AdminShutdown,
    );

    assert!(decision.is_allowed());
    assert!(decision.evidence.surface_policy_allowed);
    assert!(decision.evidence.role_permission_evaluated);
    assert!(!decision.evidence.role_permission_granted);
    assert!(decision.evidence.direct_permission_evaluated);
    assert!(decision.evidence.direct_permission_granted);
}

#[test]
fn test_principal_registry_registration_is_immutable_and_idempotent() {
    let fingerprint = seeded_fingerprint('7');
    let certificate = certificate_identity(
        fingerprint.clone(),
        "CN=svc-admin-immutable",
        SurfaceScope::Administration,
    );
    let principal = principal_for_certificate(
        &certificate,
        820,
        PrincipalRole::User,
        PrincipalStatus::Active,
    );
    let binding =
        PrincipalBinding::new(certificate.clone(), principal.clone()).expect("valid binding");

    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding.clone())
        .expect("initial binding registered");
    registry
        .register(binding)
        .expect("identical binding registration is idempotent");
    assert_eq!(registry.len(), 1);

    let changed_direct_permissions = PrincipalBinding::new_with_direct_permissions(
        certificate.clone(),
        principal.clone(),
        PermissionSet::new().with_permission(Permission::AdminShutdown),
    )
    .expect("direct admin permission is valid for Administration surface");
    assert!(
        registry.register(changed_direct_permissions).is_err(),
        "same certificate fingerprint must not silently mutate direct permissions"
    );

    let changed_role = principal_for_certificate(
        &certificate,
        820,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
    );
    let changed_role_binding =
        PrincipalBinding::new(certificate, changed_role).expect("valid binding");
    assert!(
        registry.register(changed_role_binding).is_err(),
        "same certificate fingerprint must not silently mutate role evidence"
    );
}

#[test]
fn test_role_and_direct_permissions_are_union_after_surface_and_status() {
    let fingerprint = seeded_fingerprint('f');
    let binding = principal_binding_with_direct_permissions(
        fingerprint.clone(),
        "CN=svc-admin-union",
        SurfaceScope::Administration,
        818,
        PrincipalRole::User,
        PrincipalStatus::Active,
        PermissionSet::new().with_permission(Permission::AdminShutdown),
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");

    let role_decision = registry.authorize(
        SurfaceScope::Administration,
        &fingerprint,
        &Permission::ReadContractMetadata,
    );
    assert!(role_decision.is_allowed());
    assert!(role_decision.evidence.role_permission_granted);
    assert!(!role_decision.evidence.direct_permission_granted);

    let direct_decision = registry.authorize(
        SurfaceScope::Administration,
        &fingerprint,
        &Permission::AdminShutdown,
    );
    assert!(direct_decision.is_allowed());
    assert!(!direct_decision.evidence.role_permission_granted);
    assert!(direct_decision.evidence.direct_permission_granted);
}
