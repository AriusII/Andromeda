use andromeda_core::{
    PRINCIPAL_POLICY_EVIDENCE_VERSION, PrincipalPolicyEvidenceBinding, PrincipalPolicyVersion,
};

use crate::TraceId;

use super::*;

fn certificate() -> CertificateIdentity {
    CertificateIdentity::new(
        "sha256:observe-policy-test",
        "CN=observe-policy-test",
        SurfaceScope::Application,
    )
    .expect("test certificate evidence is explicit")
}

fn principal() -> UserPrincipal {
    UserPrincipal::new("user:observe-policy", UserPrincipalKind::Service)
        .expect("test principal evidence is explicit")
}

#[test]
fn security_policy_evidence_converts_from_core_binding() {
    let core_policy_version = PrincipalPolicyVersion::test_vector(0x2a);
    let evidence = SecurityPolicyVersionEvidence::from_core_policy_version(core_policy_version)
        .expect("core policy version converts to audit policy evidence");

    assert_eq!(evidence.policy_version, PRINCIPAL_POLICY_EVIDENCE_VERSION);
    assert!(evidence.matches_core_policy_version(core_policy_version));
    assert!(!evidence.matches_core_policy_version(PrincipalPolicyVersion::test_vector(0x2b)));

    let wrong_version =
        PrincipalPolicyEvidenceBinding::from_principal_policy_version(2, core_policy_version)
            .expect("alternate policy evidence remains well-formed");
    assert!(!evidence.matches_core_policy_version_and_digest(&wrong_version));

    let wrong_digest = PrincipalPolicyVersion::test_vector(0x2b)
        .evidence_binding()
        .expect("alternate digest evidence is well-formed");
    assert!(!evidence.matches_core_policy_version_and_digest(&wrong_digest));
}

#[test]
fn security_policy_evidence_rejects_version_only_or_digest_only() {
    let valid_digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    assert!(SecurityPolicyVersionEvidence::new(0, valid_digest).is_err());
    assert!(SecurityPolicyVersionEvidence::new(1, "").is_err());
    assert!(
        SecurityPolicyVersionEvidence::new(
            1,
            "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .is_err(),
        "audit policy evidence uses the same canonical digest rule as core"
    );

    let version_only = SecurityPolicyVersionEvidence {
        policy_version: 1,
        policy_digest: String::new(),
    };
    let err = SecurityAuditTrace::new_with_policy_version(
        TraceId::new(901),
        SurfaceScope::Application,
        certificate(),
        principal(),
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Denied,
        version_only,
        "version-only policy evidence is not audit-ready",
    )
    .unwrap_err();
    assert!(err.message().contains("policy version evidence"));
}
