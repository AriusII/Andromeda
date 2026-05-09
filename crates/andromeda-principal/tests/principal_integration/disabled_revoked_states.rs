use andromeda_principal::{
    CertificateIdentityStatus, Permission, PermissionSet, PrincipalAuthorizationDenialReason,
    PrincipalId, PrincipalRegistry, PrincipalRole, PrincipalStatus, SurfaceScope,
};
use andromeda_types::ProcedureId;

use crate::principal_fixtures::{
    principal_binding, principal_binding_with_direct_permissions, test_fingerprint,
};

#[test]
fn test_disabled_user_principal_is_denied_before_permission_match() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-disabled",
        SurfaceScope::Application,
        707,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Disabled,
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
        Some(PrincipalAuthorizationDenialReason::PrincipalDisabled)
    );
    assert!(decision.evidence.has_identity_evidence());
    assert!(decision.evidence.has_reason());
    assert_eq!(decision.evidence.reason, "principal_disabled");
}

#[test]
fn test_direct_permissions_do_not_override_disabled_principal() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding_with_direct_permissions(
        fingerprint.clone(),
        "CN=svc-disabled-direct",
        SurfaceScope::Administration,
        812,
        PrincipalRole::User,
        PrincipalStatus::Disabled,
        PermissionSet::new().with_permission(Permission::AdminShutdown),
    );

    assert!(
        !binding.grants(&Permission::AdminShutdown),
        "direct permissions must not bypass disabled principal status"
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
        Some(PrincipalAuthorizationDenialReason::PrincipalDisabled)
    );
    assert_eq!(
        decision.evidence.principal_status,
        Some(PrincipalStatus::Disabled)
    );
    assert!(!decision.evidence.direct_permission_evaluated);
}

#[test]
fn test_certificate_revocation_and_principal_disable_remain_distinct_statuses() {
    let fingerprint = test_fingerprint();
    let binding = principal_binding(
        fingerprint.clone(),
        "CN=svc-status-chain",
        SurfaceScope::Application,
        813,
        PrincipalRole::User,
        PrincipalStatus::Active,
    );

    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("binding registered");
    registry
        .revoke_certificate(&fingerprint)
        .expect("certificate revoked");

    let revoked_binding = registry.lookup(&fingerprint).expect("binding retained");
    assert_eq!(
        revoked_binding.certificate().status(),
        CertificateIdentityStatus::Revoked
    );
    assert_eq!(revoked_binding.principal().status, PrincipalStatus::Active);

    registry
        .disable_principal(PrincipalId::new(813))
        .expect("principal disabled");

    let disabled_binding = registry.lookup(&fingerprint).expect("binding retained");
    assert_eq!(
        disabled_binding.certificate().status(),
        CertificateIdentityStatus::Revoked
    );
    assert_eq!(
        disabled_binding.principal().status,
        PrincipalStatus::Disabled
    );
}
