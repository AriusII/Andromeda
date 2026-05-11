use andromeda_audit::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
use andromeda_hadr::{
    HadrClusterManifestUpdateEvidence, HadrClusterManifestUpdateRequest,
    HadrClusterManifestVersion, HadrClusterOperation, HadrClusterSecurityEvidence, HadrEpoch,
};
use andromeda_observability::TraceId;

fn audit(reason: impl Into<String>) -> SecurityAuditTrace {
    SecurityAuditTrace::new_with_policy_version(
        TraceId::new(42_101),
        SurfaceScope::Cluster,
        CertificateIdentity::new("fp-hadr", "CN=hadr-controller", SurfaceScope::Cluster)
            .expect("certificate"),
        UserPrincipal::new("svc-hadr", UserPrincipalKind::Service).expect("principal"),
        Permission::UpdateClusterManifest,
        SecurityAuditOutcome::Allowed,
        SecurityPolicyVersionEvidence::new(
            10,
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .expect("policy evidence"),
        reason,
    )
    .expect("security audit")
}

fn update_manifest_security() -> HadrClusterSecurityEvidence {
    HadrClusterSecurityEvidence::new(HadrClusterOperation::UpdateManifest, audit("safe reason"))
        .expect("manifest security")
}

#[test]
fn cluster_security_evidence_rejects_secret_bearing_or_unbounded_reasons() {
    let secret_reason = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::UpdateManifest,
        audit("rotation failed token=abc"),
    )
    .expect_err("cluster security must reject secret-bearing reasons");
    assert!(secret_reason.message().contains("sensitive markers"));

    let long_reason = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::UpdateManifest,
        audit("a".repeat(513)),
    )
    .expect_err("cluster security must reject overlong reason evidence");
    assert!(long_reason.message().contains("bounded evidence length"));
}

#[test]
fn manifest_update_request_rejects_secret_bearing_or_unbounded_reasons() {
    let secret_request = HadrClusterManifestUpdateRequest::new(
        HadrEpoch::new(7),
        HadrEpoch::new(8),
        HadrClusterManifestVersion::new(12),
        HadrClusterManifestVersion::new(13),
        2,
        2,
        "manual override password=abc",
    );
    let secret_error =
        HadrClusterManifestUpdateEvidence::new(secret_request, update_manifest_security())
            .expect_err("manifest request must reject secret-bearing reason");
    assert!(secret_error.message().contains("sensitive markers"));

    let long_request = HadrClusterManifestUpdateRequest::new(
        HadrEpoch::new(7),
        HadrEpoch::new(8),
        HadrClusterManifestVersion::new(12),
        HadrClusterManifestVersion::new(13),
        2,
        2,
        "b".repeat(513),
    );
    let long_error =
        HadrClusterManifestUpdateEvidence::new(long_request, update_manifest_security())
            .expect_err("manifest request must reject overlong reason");
    assert!(long_error.message().contains("bounded evidence length"));
}
