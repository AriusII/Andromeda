use andromeda_audit::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
use andromeda_hadr::{
    HadrClusterDurableAuditProof, HadrClusterOperation, HadrClusterSecurityEvidence, HadrEpoch,
    HadrFencingContext, HadrFencingToken, HadrNodeId, HadrSecuredFencingEvidence,
    HadrSecuredFencingRequest, require_cluster_fence_admission,
    require_cluster_manifest_update_admission, require_cluster_promotion_admission,
    require_wal_shipping_control_admission,
};
use andromeda_observability::TraceId;
use andromeda_wal::Lsn;

fn audit(
    surface: SurfaceScope,
    permission: Permission,
    outcome: SecurityAuditOutcome,
) -> SecurityAuditTrace {
    SecurityAuditTrace::new_with_policy_version(
        TraceId::new(42_001),
        surface,
        CertificateIdentity::new("fp-hadr", "CN=hadr-controller", surface).expect("certificate"),
        UserPrincipal::new("svc-hadr", UserPrincipalKind::Service).expect("principal"),
        permission,
        outcome,
        SecurityPolicyVersionEvidence::new(
            9,
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("policy evidence"),
        "authorized cluster operation",
    )
    .expect("security audit")
}

fn proof() -> HadrClusterDurableAuditProof {
    HadrClusterDurableAuditProof::new(
        Lsn::new(17),
        [
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
    )
    .expect("durable proof")
}

fn fence_security() -> HadrClusterSecurityEvidence {
    HadrClusterSecurityEvidence::new(
        HadrClusterOperation::FenceNode,
        audit(
            SurfaceScope::Cluster,
            Permission::FenceNode,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect("fence security")
}

fn active_fencing_context() -> HadrFencingContext {
    HadrFencingContext::with_active(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(7)),
        HadrEpoch::new(7),
    )
}

fn secured_fencing_request() -> HadrSecuredFencingRequest {
    HadrSecuredFencingRequest::new(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(7)),
        2,
        2,
        "quorum fenced stale primary",
    )
}

#[test]
fn cluster_operation_admission_is_fail_closed() {
    let wrong_surface = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        audit(
            SurfaceScope::Administration,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect_err("cluster operation must reject non-cluster surface");
    assert!(wrong_surface.message().contains("Cluster surface"));

    let wrong_permission = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        audit(
            SurfaceScope::Cluster,
            Permission::FenceNode,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect_err("cluster operation must reject wrong permission");
    assert!(wrong_permission.message().contains("ClusterPromote"));

    let denied = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::FenceNode,
        audit(
            SurfaceScope::Cluster,
            Permission::FenceNode,
            SecurityAuditOutcome::Denied,
        ),
    )
    .expect_err("cluster operation must reject denied outcome");
    assert!(denied.message().contains("allowed decision"));

    let mut no_policy = audit(
        SurfaceScope::Cluster,
        Permission::UpdateClusterManifest,
        SecurityAuditOutcome::Allowed,
    );
    no_policy.policy_version.policy_version = 0;
    let no_policy_error =
        HadrClusterSecurityEvidence::new(HadrClusterOperation::UpdateManifest, no_policy)
            .expect_err("cluster operation must reject missing policy version evidence");
    assert!(
        no_policy_error
            .message()
            .contains("policy version evidence")
    );

    let mut no_identity = audit(
        SurfaceScope::Cluster,
        Permission::UpdateClusterManifest,
        SecurityAuditOutcome::Allowed,
    );
    no_identity.principal.principal_id = " ".to_string();
    let no_identity_error =
        HadrClusterSecurityEvidence::new(HadrClusterOperation::UpdateManifest, no_identity)
            .expect_err("cluster operation must reject missing principal identity");
    assert!(no_identity_error.message().contains("principal evidence"));
}

#[test]
fn durable_admission_proof_is_required_when_represented() {
    let promotion_security = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        audit(
            SurfaceScope::Cluster,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect("promotion security");

    require_cluster_promotion_admission(&promotion_security, Some(&proof()))
        .expect("promotion should admit with durable proof");

    let missing_proof = require_cluster_promotion_admission(&promotion_security, None)
        .expect_err("promotion must reject when durable proof is missing");
    assert!(missing_proof.message().contains("durable audit proof"));

    let zero_lsn = HadrClusterDurableAuditProof::new(
        Lsn::ZERO,
        [
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
    )
    .expect_err("durable proof must require non-zero audit LSN");
    assert!(zero_lsn.message().contains("non-zero audit LSN"));

    let zero_digest = HadrClusterDurableAuditProof::new(Lsn::new(17), [0; 32])
        .expect_err("durable proof must require non-zero digest");
    assert!(zero_digest.message().contains("non-zero marker digest"));
}

#[test]
fn operation_specific_admission_helpers_reject_mismatched_security() {
    let fence_security = fence_security();
    require_cluster_fence_admission(&fence_security, Some(&proof()))
        .expect("fence admission should succeed");

    let promotion_security = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        audit(
            SurfaceScope::Cluster,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect("promotion security");
    let mismatch = require_cluster_manifest_update_admission(&promotion_security)
        .expect_err("manifest admission must reject non-manifest operation evidence");
    assert!(mismatch.message().contains("cluster manifest update"));
}

#[test]
fn secured_fencing_rejects_wrong_operation_before_visible_mutation() {
    let wrong_security = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        audit(
            SurfaceScope::Cluster,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect("promotion security");

    let error = HadrSecuredFencingEvidence::new(
        secured_fencing_request(),
        &active_fencing_context(),
        wrong_security,
        Some(&proof()),
    )
    .expect_err("secured fencing must reject non-fencing security evidence");

    assert!(error.message().contains("cluster node fencing"));
}

#[test]
fn secured_fencing_rejects_missing_or_invalid_durable_proof() {
    let missing = HadrSecuredFencingEvidence::new(
        secured_fencing_request(),
        &active_fencing_context(),
        fence_security(),
        None,
    )
    .expect_err("secured fencing must reject missing durable audit proof");
    assert!(missing.message().contains("durable audit proof"));

    let invalid_hash = HadrClusterDurableAuditProof::new(Lsn::new(18), [0; 32])
        .expect_err("durable proof must reject missing marker hash");
    assert!(invalid_hash.message().contains("non-zero marker digest"));
}

#[test]
fn secured_fencing_rejects_invalid_stale_token_and_insufficient_quorum() {
    let invalid_token = HadrSecuredFencingRequest::new(
        HadrFencingToken::new(HadrNodeId::new(0), HadrEpoch::new(7)),
        2,
        2,
        "invalid primary id",
    );
    let invalid = HadrSecuredFencingEvidence::new(
        invalid_token,
        &active_fencing_context(),
        fence_security(),
        Some(&proof()),
    )
    .expect_err("secured fencing must reject invalid token identity");
    assert!(invalid.message().contains("non-zero primary id"));

    let stale_token = HadrSecuredFencingRequest::new(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(6)),
        2,
        2,
        "stale token",
    );
    let stale = HadrSecuredFencingEvidence::new(
        stale_token,
        &active_fencing_context(),
        fence_security(),
        Some(&proof()),
    )
    .expect_err("secured fencing must reject stale fencing token");
    assert!(stale.message().contains("below active epoch"));

    let future_token = HadrSecuredFencingRequest::new(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(8)),
        2,
        2,
        "future epoch token",
    );
    let future = HadrSecuredFencingEvidence::new(
        future_token,
        &active_fencing_context(),
        fence_security(),
        Some(&proof()),
    )
    .expect_err("secured fencing must reject future fencing token");
    assert!(future.message().contains("possible split-brain evidence"));

    let insufficient_quorum = HadrSecuredFencingRequest::new(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(7)),
        2,
        1,
        "insufficient quorum",
    );
    let quorum = HadrSecuredFencingEvidence::new(
        insufficient_quorum,
        &active_fencing_context(),
        fence_security(),
        Some(&proof()),
    )
    .expect_err("secured fencing must reject missing quorum evidence");
    assert!(quorum.message().contains("quorum-granted evidence"));
}

#[test]
fn secured_fencing_rejects_sensitive_reason_markers() {
    let sensitive_reason = HadrSecuredFencingRequest::new(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(7)),
        2,
        2,
        "token=cluster-secret",
    );

    let error = HadrSecuredFencingEvidence::new(
        sensitive_reason,
        &active_fencing_context(),
        fence_security(),
        Some(&proof()),
    )
    .expect_err("secured fencing must reject secret-bearing reason evidence");

    assert!(error.message().contains("sensitive markers"));
}

#[test]
fn secured_fencing_accepts_authorized_quorum_token_with_durable_audit() {
    let evidence = HadrSecuredFencingEvidence::new(
        secured_fencing_request(),
        &active_fencing_context(),
        fence_security(),
        Some(&proof()),
    )
    .expect("secured fencing should admit fully evidenced mutation");

    assert_eq!(
        evidence.active_token(),
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(7))
    );
    assert_eq!(evidence.request().granted_votes, 2);
    assert_eq!(
        evidence.security().operation(),
        HadrClusterOperation::FenceNode
    );
    assert_eq!(evidence.durable_audit().audit_lsn(), Lsn::new(17));
}

#[test]
fn wal_shipping_control_requires_cluster_permission_and_durable_proof() {
    let durable_proof = proof();
    let allowed_audit = audit(
        SurfaceScope::Cluster,
        Permission::UpdateClusterManifest,
        SecurityAuditOutcome::Allowed,
    );

    require_wal_shipping_control_admission(
        Permission::UpdateClusterManifest,
        &allowed_audit,
        Some(&durable_proof),
    )
    .expect("wal shipping control should pass with explicit cluster permission");

    let bad_required_permission = require_wal_shipping_control_admission(
        Permission::DebugProcedure,
        &allowed_audit,
        Some(&durable_proof),
    )
    .expect_err("wal shipping control must reject non-cluster required permission");
    assert!(
        bad_required_permission
            .message()
            .contains("explicit cluster admin permission")
    );

    let denied_audit = audit(
        SurfaceScope::Cluster,
        Permission::UpdateClusterManifest,
        SecurityAuditOutcome::Denied,
    );
    let denied = require_wal_shipping_control_admission(
        Permission::UpdateClusterManifest,
        &denied_audit,
        Some(&durable_proof),
    )
    .expect_err("wal shipping control must reject denied outcome");
    assert!(denied.message().contains("allowed decision"));

    let mut stale = allowed_audit.clone();
    stale.trace_id = TraceId::new(0);
    let stale_error = require_wal_shipping_control_admission(
        Permission::UpdateClusterManifest,
        &stale,
        Some(&durable_proof),
    )
    .expect_err("wal shipping control must reject stale/zero trace ids");
    assert!(stale_error.message().contains("non-zero trace id"));

    let no_proof = require_wal_shipping_control_admission(
        Permission::UpdateClusterManifest,
        &allowed_audit,
        None,
    )
    .expect_err("wal shipping control must reject missing durable proof");
    assert!(no_proof.message().contains("durable audit proof"));
}
