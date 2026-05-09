use super::*;
use andromeda_audit::{
    AdminOperation, CertificateIdentity, Permission, SecurityAuditOutcome, SurfaceScope,
    UserPrincipal, UserPrincipalKind,
};
use andromeda_observability::TraceId;

fn cert(fingerprint: &str, surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(fingerprint, format!("CN={}", fingerprint), surface).unwrap()
}

fn principal(id: &str) -> UserPrincipal {
    UserPrincipal::new(id, UserPrincipalKind::Service).unwrap()
}

fn binding(
    fingerprint: &str,
    surface: SurfaceScope,
    principal_id: &str,
    permissions: Vec<Permission>,
) -> PrincipalBinding {
    PrincipalBinding::new(
        cert(fingerprint, surface),
        principal(principal_id),
        permissions,
    )
    .unwrap()
}

fn registry_with(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
    let mut registry = PrincipalRegistry::new();
    for b in bindings {
        registry.register(b).unwrap();
    }
    registry
}

#[test]
fn binding_dedupes_repeated_permission_grants() {
    let b = binding(
        "fp-app-1",
        SurfaceScope::Application,
        "svc-1",
        vec![
            Permission::ExecuteProcedure,
            Permission::ExecuteProcedure,
            Permission::ReadContract,
        ],
    );
    assert_eq!(b.permissions().len(), 2);
}

#[test]
fn registry_rejects_fingerprint_collision_with_different_principal() {
    let mut reg = PrincipalRegistry::new();
    reg.register(binding("fp-1", SurfaceScope::Application, "svc-a", vec![]))
        .unwrap();
    let err = reg
        .register(binding("fp-1", SurfaceScope::Application, "svc-b", vec![]))
        .unwrap_err();
    assert!(format!("{err}").contains("rotation"));
}

#[test]
fn registry_allows_idempotent_re_register_for_same_principal() {
    let mut reg = PrincipalRegistry::new();
    reg.register(binding(
        "fp-1",
        SurfaceScope::Application,
        "svc-a",
        vec![Permission::ExecuteProcedure],
    ))
    .unwrap();
    reg.register(binding(
        "fp-1",
        SurfaceScope::Application,
        "svc-a",
        vec![Permission::ExecuteProcedure, Permission::ReadContract],
    ))
    .unwrap();
    assert_eq!(reg.len(), 1);
    assert_eq!(reg.lookup("fp-1").unwrap().permissions().len(), 2);
}

#[test]
fn unknown_certificate_denies_with_audit_evidence() {
    let reg = registry_with(vec![]);
    let auth = SurfaceAuthorizer::new(&reg);

    let outcome = auth
        .authorize(
            TraceId::new(1),
            SurfaceScope::Application,
            "fp-missing",
            SurfaceAction::ExecuteProcedure,
        )
        .unwrap();

    assert!(outcome.is_denied());
    let audit = outcome.audit();
    assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
    assert!(audit.has_reason());
    assert!(audit.has_supported_schema_version());
    assert!(audit.has_identity_evidence());
    if let AuthorizationOutcome::Denied { reason, .. } = outcome {
        assert_eq!(reason, AuthorizationDenialReason::UnknownCertificate);
    }
}

#[test]
fn surface_scope_mismatch_denies_even_with_permission_grant() {
    let reg = registry_with(vec![binding(
        "fp-app-1",
        SurfaceScope::Application,
        "svc-1",
        vec![Permission::ExecuteProcedure],
    )]);
    let auth = SurfaceAuthorizer::new(&reg);

    let outcome = auth
        .authorize(
            TraceId::new(2),
            SurfaceScope::Administration,
            "fp-app-1",
            SurfaceAction::ExecuteProcedure,
        )
        .unwrap();

    assert!(outcome.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = outcome {
        assert_eq!(reason, AuthorizationDenialReason::SurfaceScopeMismatch);
        assert!(!audit.surface_matches_certificate());
    }
}

#[test]
fn surface_does_not_permit_admin_permission_on_application() {
    let reg = registry_with(vec![binding(
        "fp-app-1",
        SurfaceScope::Application,
        "svc-1",
        vec![Permission::ManageSecurity],
    )]);
    let auth = SurfaceAuthorizer::new(&reg);

    let outcome = auth
        .authorize(
            TraceId::new(3),
            SurfaceScope::Application,
            "fp-app-1",
            SurfaceAction::Admin(AdminOperation::ManageSecurity),
        )
        .unwrap();

    assert!(outcome.is_denied());
    if let AuthorizationOutcome::Denied { reason, .. } = outcome {
        assert_eq!(
            reason,
            AuthorizationDenialReason::SurfaceDoesNotPermitPermission
        );
    }
}

#[test]
fn principal_missing_permission_denies_with_typed_reason() {
    let reg = registry_with(vec![binding(
        "fp-adm-1",
        SurfaceScope::Administration,
        "ops-1",
        vec![Permission::Backup],
    )]);
    let auth = SurfaceAuthorizer::new(&reg);

    let outcome = auth
        .authorize(
            TraceId::new(4),
            SurfaceScope::Administration,
            "fp-adm-1",
            SurfaceAction::Admin(AdminOperation::ManageSecurity),
        )
        .unwrap();

    assert!(outcome.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = outcome {
        assert_eq!(
            reason,
            AuthorizationDenialReason::PrincipalMissingPermission
        );
        assert!(audit.permission == Permission::ManageSecurity);
    }
}

#[test]
fn happy_path_allows_and_emits_audit_for_dispatch() {
    let reg = registry_with(vec![binding(
        "fp-app-1",
        SurfaceScope::Application,
        "svc-1",
        vec![Permission::ExecuteProcedure, Permission::ReadContract],
    )]);
    let auth = SurfaceAuthorizer::new(&reg);

    let outcome = auth
        .authorize(
            TraceId::new(5),
            SurfaceScope::Application,
            "fp-app-1",
            SurfaceAction::ExecuteProcedure,
        )
        .unwrap();

    assert!(outcome.is_allowed());
    let audit = outcome.audit();
    assert_eq!(audit.outcome, SecurityAuditOutcome::Allowed);
    assert!(audit.has_reason());
    assert!(audit.has_supported_schema_version());
    assert!(audit.has_identity_evidence());
    assert!(audit.surface_matches_certificate());
    assert!(audit.surface_permits_permission());
    assert!(!audit.contains_sensitive_evidence());

    if let AuthorizationOutcome::Allowed {
        principal,
        permission,
        ..
    } = outcome
    {
        assert_eq!(principal.principal_id, "svc-1");
        assert_eq!(permission, Permission::ExecuteProcedure);
    }
}

#[test]
fn admin_action_required_permission_matches_admin_op() {
    for op in [
        AdminOperation::DebugProcedure,
        AdminOperation::ManageSecurity,
        AdminOperation::Backup,
        AdminOperation::ClusterPromote,
    ] {
        let action = SurfaceAction::Admin(op);
        assert_eq!(action.required_permission(), op.required_permission());
        assert!(action.is_admin());
    }
}

#[test]
fn fingerprint_evidence_is_sanitized_for_unknown_certs() {
    let reg = registry_with(vec![]);
    let auth = SurfaceAuthorizer::new(&reg);

    let outcome = auth
        .authorize(
            TraceId::new(6),
            SurfaceScope::Application,
            "   ",
            SurfaceAction::ExecuteProcedure,
        )
        .unwrap();
    assert!(outcome.is_denied());
    let audit = outcome.audit();
    assert!(audit.certificate.fingerprint.starts_with("fingerprint:"));
    assert!(!audit.contains_sensitive_evidence());
}
