use andromeda_audit::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
use andromeda_hadr::{
    HadrClusterDurableAuditProof, HadrClusterOperation, HadrClusterSecurityEvidence,
    require_cluster_fence_admission, require_cluster_manifest_update_admission,
    require_cluster_promotion_admission, require_wal_shipping_control_admission,
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
    let fence_security = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::FenceNode,
        audit(
            SurfaceScope::Cluster,
            Permission::FenceNode,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect("fence security");
    require_cluster_fence_admission(&fence_security).expect("fence admission should succeed");

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
