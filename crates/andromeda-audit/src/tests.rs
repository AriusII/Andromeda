use andromeda_security_contract::{
    SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION, SecurityPolicyEvidence, SecurityPolicyVersion,
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
fn security_policy_evidence_converts_from_security_contract() {
    let policy_version = SecurityPolicyVersion::test_vector(0x2a);
    let evidence = SecurityPolicyVersionEvidence::from_security_policy_version(policy_version)
        .expect("security policy version converts to audit policy evidence");

    assert_eq!(
        evidence.policy_version,
        SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION
    );
    assert!(evidence.matches_security_policy_version(policy_version));
    assert!(!evidence.matches_security_policy_version(SecurityPolicyVersion::test_vector(0x2b)));

    let wrong_version = SecurityPolicyEvidence::new(2, SecurityPolicyVersion::test_vector(0x2a))
        .expect("alternate schema version remains well-formed");
    assert!(!evidence.matches_security_policy_evidence(&wrong_version));

    let wrong_digest = SecurityPolicyEvidence::new(
        SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION,
        SecurityPolicyVersion::test_vector(0x2b),
    )
    .expect("alternate digest remains well-formed");
    assert!(!evidence.matches_security_policy_evidence(&wrong_digest));
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
