use andromeda_core::{
    CertificateIdentityStatus, HardwareArchitecture, HardwareProfile,
    PRINCIPAL_POLICY_EVIDENCE_VERSION, Permission, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage, PrincipalBinding, PrincipalPolicyEvidenceBinding,
    PrincipalPolicyVersion, PrincipalRegistry, PrincipalRole, PrincipalStatus, ProcedureId,
    ResourceBudget, SurfaceScope,
};

#[allow(dead_code)]
#[path = "support/principal_fixtures.rs"]
mod principal_fixtures;

use principal_fixtures::{principal_binding, seeded_fingerprint};

fn binding(
    fingerprint: String,
    surface_scope: SurfaceScope,
    role: PrincipalRole,
    status: PrincipalStatus,
) -> PrincipalBinding {
    principal_binding(
        fingerprint,
        "CN=resource-policy",
        surface_scope,
        900,
        role,
        status,
    )
}

#[test]
fn conservative_hardware_profile_keeps_accelerators_out_of_required_path() {
    let profile = HardwareProfile::conservative();

    assert_eq!(profile.architecture, HardwareArchitecture::Unknown);
    assert!(!profile.has_simd);
    assert!(!profile.has_direct_io);
    assert_eq!(HardwareProfile::default(), profile);
}

#[test]
fn resource_budget_preserves_explicit_ram_temp_and_stream_limits() {
    let budget = ResourceBudget::new(64 * 1024 * 1024, 256 * 1024 * 1024, 8);

    assert_eq!(budget.max_memory_bytes, 64 * 1024 * 1024);
    assert_eq!(budget.max_temp_bytes, 256 * 1024 * 1024);
    assert_eq!(budget.max_streams, 8);
}

#[test]
fn iam_policy_gate_allows_active_principal_on_matching_surface() {
    let fp = seeded_fingerprint('a');
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Application,
            PrincipalRole::User,
            PrincipalStatus::Active,
        ))
        .expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_allowed());
    assert_eq!(decision.evidence.reason, "allowed");
    assert!(decision.evidence.has_identity_evidence());
    assert!(decision.evidence.has_policy_version());
    assert!(decision.evidence.is_audit_ready());
}

#[test]
fn iam_policy_gate_denies_disabled_principal_even_when_role_would_allow() {
    let fp = seeded_fingerprint('b');
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Application,
            PrincipalRole::SuperAdmin,
            PrincipalStatus::Disabled,
        ))
        .expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::PrincipalDisabled)
    );
    assert_eq!(decision.evidence.reason, "principal_disabled");
    assert!(decision.evidence.has_policy_version());
    assert!(decision.evidence.is_audit_ready());
}

#[test]
fn iam_policy_gate_denies_surface_scope_mismatch_before_role_permission() {
    let fp = seeded_fingerprint('c');
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Administration,
            PrincipalRole::SuperAdmin,
            PrincipalStatus::Active,
        ))
        .expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::SurfaceScopeMismatch)
    );
    assert_eq!(decision.evidence.reason, "surface_scope_mismatch");
    assert!(decision.evidence.has_policy_version());
    assert!(decision.evidence.is_audit_ready());
}

#[test]
fn iam_policy_gate_denies_revoked_certificate_identity() {
    let fp = seeded_fingerprint('d');
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Application,
            PrincipalRole::User,
            PrincipalStatus::Active,
        ))
        .expect("binding registered");
    registry
        .revoke_certificate(&fp)
        .expect("certificate identity revoked");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::CertificateRevoked)
    );
    assert_eq!(decision.evidence.reason, "certificate_revoked");
    assert!(decision.evidence.has_policy_version());
    assert!(decision.evidence.is_audit_ready());
}

#[test]
fn principal_iam_policy_gate_denies_disabled_certificate_identity_with_audit_evidence() {
    let fp = seeded_fingerprint('8');
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Application,
            PrincipalRole::SuperAdmin,
            PrincipalStatus::Active,
        ))
        .expect("binding registered");
    registry
        .disable_certificate(&fp)
        .expect("certificate identity disabled");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_denied());
    assert_eq!(
        decision.denial_reason,
        Some(PrincipalAuthorizationDenialReason::CertificateDisabled)
    );
    assert_eq!(decision.evidence.reason, "certificate_disabled");
    assert_eq!(
        decision.evidence.certificate_status,
        Some(CertificateIdentityStatus::Disabled)
    );
    assert_eq!(
        decision.evidence.evaluation_stage(),
        PrincipalAuthorizationEvaluationStage::CertificateStatus
    );
    assert!(!decision.evidence.role_permission_evaluated);
    assert!(!decision.evidence.direct_permission_evaluated);
    assert!(decision.evidence.has_policy_version());
    assert!(decision.evidence.is_audit_ready());
}

#[test]
fn iam_policy_gate_binds_explicit_policy_version_to_authorization_evidence() {
    let policy_version = PrincipalPolicyVersion::test_vector(0x5a);
    let fp = seeded_fingerprint('e');
    let mut registry =
        PrincipalRegistry::new_with_policy_version(policy_version).expect("policy version bound");
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Application,
            PrincipalRole::User,
            PrincipalStatus::Active,
        ))
        .expect("binding registered");

    let allowed = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    assert!(allowed.is_allowed());
    assert_eq!(allowed.evidence.policy_version, policy_version);
    assert_eq!(registry.policy_version(), policy_version);
    assert!(allowed.evidence.is_audit_ready());

    let denied = registry.authorize(SurfaceScope::Application, &fp, &Permission::AdminShutdown);
    assert!(denied.is_denied());
    assert_eq!(denied.evidence.policy_version, policy_version);
    assert_eq!(
        denied.denial_reason,
        Some(PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission)
    );
    assert!(denied.evidence.is_audit_ready());
}

#[test]
fn iam_policy_gate_rejects_zero_policy_version() {
    let registry = PrincipalRegistry::new_with_policy_version(PrincipalPolicyVersion::zero());

    assert!(registry.is_err());
}

#[test]
fn iam_policy_evidence_binding_requires_version_and_digest() {
    let valid_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    assert!(PrincipalPolicyEvidenceBinding::new(0, valid_digest).is_err());
    assert!(PrincipalPolicyEvidenceBinding::new(1, "").is_err());
    assert!(
        PrincipalPolicyEvidenceBinding::new(
            1,
            "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .is_err(),
        "canonical digest evidence is lower-case sha256 hex"
    );
    assert!(PrincipalPolicyVersion::zero().evidence_binding().is_err());
}

#[test]
fn iam_policy_evidence_binding_compares_version_and_digest_deterministically() {
    let policy_version = PrincipalPolicyVersion::test_vector(0x5a);
    let expected_digest = format!("sha256:{}", "5a".repeat(32));
    let binding = PrincipalPolicyEvidenceBinding::from_principal_policy_version(7, policy_version)
        .expect("non-zero policy version can bind digest evidence");

    assert_eq!(binding.policy_version(), 7);
    assert_eq!(binding.policy_digest(), expected_digest);
    assert!(binding.matches_version_and_digest(7, &expected_digest));
    assert!(
        !binding.matches_version_and_digest(PRINCIPAL_POLICY_EVIDENCE_VERSION, &expected_digest)
    );
    assert!(
        !binding.matches_principal_policy_version(policy_version),
        "non-current evidence version must not match audit-ready core evidence"
    );

    let current_binding = policy_version
        .evidence_binding()
        .expect("non-zero policy version has canonical evidence");
    assert_eq!(
        current_binding.policy_version(),
        PRINCIPAL_POLICY_EVIDENCE_VERSION
    );
    assert!(current_binding.matches_principal_policy_version(policy_version));
    assert!(
        !current_binding
            .matches_principal_policy_version(PrincipalPolicyVersion::test_vector(0x5b))
    );
}

#[test]
fn iam_authorization_evidence_exposes_audit_policy_binding() {
    let policy_version = PrincipalPolicyVersion::test_vector(0x6c);
    let fp = seeded_fingerprint('f');
    let mut registry =
        PrincipalRegistry::new_with_policy_version(policy_version).expect("policy version bound");
    registry
        .register(binding(
            fp.clone(),
            SurfaceScope::Application,
            PrincipalRole::User,
            PrincipalStatus::Active,
        ))
        .expect("binding registered");

    let decision = registry.authorize(
        SurfaceScope::Application,
        &fp,
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    let expected_binding = registry
        .policy_evidence_binding()
        .expect("registry policy evidence is canonical");

    assert!(decision.is_allowed());
    assert!(decision.evidence.is_audit_ready());
    assert!(
        decision
            .evidence
            .matches_policy_version_and_digest(&expected_binding)
    );

    let mismatched_digest = PrincipalPolicyVersion::test_vector(0x6d)
        .evidence_binding()
        .expect("mismatched policy evidence is still well-formed");
    assert!(
        !decision
            .evidence
            .matches_policy_version_and_digest(&mismatched_digest)
    );

    let mismatched_version =
        PrincipalPolicyEvidenceBinding::from_principal_policy_version(2, policy_version)
            .expect("alternate version evidence is well-formed");
    assert!(
        !decision
            .evidence
            .matches_policy_version_and_digest(&mismatched_version)
    );
}
