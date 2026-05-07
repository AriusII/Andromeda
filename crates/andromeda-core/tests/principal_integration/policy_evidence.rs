use andromeda_core::{
    Permission, PrincipalAuthorizationEvaluationStage, PrincipalRegistry, PrincipalRole,
    PrincipalStatus, ProcedureId, SurfaceScope,
};

use crate::principal_fixtures::{principal_binding, seeded_fingerprint};

fn registry_with_principal(
    fingerprint: String,
    subject: &str,
    surface_scope: SurfaceScope,
    principal_id: u64,
    role: PrincipalRole,
    status: PrincipalStatus,
) -> PrincipalRegistry {
    let mut registry = PrincipalRegistry::new();
    registry
        .register(principal_binding(
            fingerprint,
            subject,
            surface_scope,
            principal_id,
            role,
            status,
        ))
        .expect("binding registered");
    registry
}

#[test]
fn test_unknown_certificate_evidence_reports_certificate_lookup_stage() {
    let unknown = PrincipalRegistry::new().authorize(
        SurfaceScope::Application,
        "unknown-fingerprint",
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    assert_eq!(
        unknown.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::CertificateLookup
    );
    assert!(unknown.evidence.has_policy_version());
    assert!(unknown.evidence.is_audit_ready());
}

#[test]
fn test_revoked_certificate_evidence_reports_certificate_status_stage() {
    let fingerprint = seeded_fingerprint('a');
    let mut registry = registry_with_principal(
        fingerprint.clone(),
        "CN=svc-stage",
        SurfaceScope::Application,
        822,
        PrincipalRole::User,
        PrincipalStatus::Active,
    );
    registry
        .revoke_certificate(&fingerprint)
        .expect("certificate revoked");
    let revoked = registry.authorize(
        SurfaceScope::Application,
        &fingerprint,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    assert_eq!(
        revoked.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::CertificateStatus
    );
}

#[test]
fn test_surface_mismatch_evidence_reports_surface_scope_stage() {
    let surface_mismatch_fingerprint = seeded_fingerprint('b');
    let registry = registry_with_principal(
        surface_mismatch_fingerprint.clone(),
        "CN=svc-stage-admin",
        SurfaceScope::Administration,
        823,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
    );
    let surface_mismatch = registry.authorize(
        SurfaceScope::Application,
        &surface_mismatch_fingerprint,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    assert_eq!(
        surface_mismatch.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::SurfaceScope
    );
}

#[test]
fn test_disabled_principal_evidence_reports_principal_status_stage() {
    let disabled_fingerprint = seeded_fingerprint('c');
    let registry = registry_with_principal(
        disabled_fingerprint.clone(),
        "CN=svc-stage-disabled",
        SurfaceScope::Application,
        824,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Disabled,
    );
    let disabled = registry.authorize(
        SurfaceScope::Application,
        &disabled_fingerprint,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    assert_eq!(
        disabled.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::PrincipalStatus
    );
}

#[test]
fn test_surface_policy_denial_evidence_reports_surface_policy_stage() {
    let surface_policy_fingerprint = seeded_fingerprint('d');
    let registry = registry_with_principal(
        surface_policy_fingerprint.clone(),
        "CN=svc-stage-policy",
        SurfaceScope::Application,
        825,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Active,
    );
    let surface_policy = registry.authorize(
        SurfaceScope::Application,
        &surface_policy_fingerprint,
        &Permission::AdminShutdown,
    );
    assert_eq!(
        surface_policy.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::SurfacePolicy
    );
}

#[test]
fn test_missing_permission_evidence_reports_permission_stage() {
    let missing_permission_fingerprint = seeded_fingerprint('e');
    let registry = registry_with_principal(
        missing_permission_fingerprint.clone(),
        "CN=svc-stage-missing",
        SurfaceScope::Administration,
        826,
        PrincipalRole::User,
        PrincipalStatus::Active,
    );
    let missing_permission = registry.authorize(
        SurfaceScope::Administration,
        &missing_permission_fingerprint,
        &Permission::AdminShutdown,
    );
    assert_eq!(
        missing_permission.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::Permission
    );
}

#[test]
fn test_allowed_authorization_evidence_reports_allowed_stage() {
    let allowed_fingerprint = seeded_fingerprint('e');
    let registry = registry_with_principal(
        allowed_fingerprint.clone(),
        "CN=svc-stage-allowed",
        SurfaceScope::Administration,
        826,
        PrincipalRole::User,
        PrincipalStatus::Active,
    );

    let allowed = registry.authorize(
        SurfaceScope::Administration,
        &allowed_fingerprint,
        &Permission::ReadContractMetadata,
    );
    assert_eq!(
        allowed.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::Allowed
    );
}
