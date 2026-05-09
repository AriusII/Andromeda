use std::cell::RefCell;

use andromeda_core::{AndromedaError, AndromedaErrorKind};
use andromeda_hadr::{
    FileBackedHadrMembershipStore, HadrClusterOperation, HadrClusterSecurityEvidence, HadrEpoch,
    HadrFencingContext, HadrFencingToken, HadrMembershipRecord, HadrMembershipStore, HadrNodeId,
    HadrNodeRole, HadrPromotionAuditLog, HadrPromotionAuditMarker, HadrPromotionAuditReceipt,
    HadrPromotionVote, NoopPromotionAuditLog, PromotionAttempt, PromotionBoundary,
};
use andromeda_observe::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, TraceId, UserPrincipal, UserPrincipalKind,
};
use andromeda_wal::Lsn;

fn store_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("hadr-membership.store")
}

fn setup_three_node_store() -> (tempfile::TempDir, FileBackedHadrMembershipStore) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FileBackedHadrMembershipStore::open(store_path(&dir)).expect("open store");
    store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica 2");
    store
        .register_node(HadrNodeId::new(3), HadrNodeRole::Replica)
        .expect("register replica 3");
    (dir, store)
}

fn fencing_at_epoch(epoch: u64) -> HadrFencingContext {
    HadrFencingContext::with_active(
        HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(epoch)),
        HadrEpoch::new(epoch),
    )
}

fn granting_votes(epoch: u64, safe_lsn: u64) -> Vec<HadrPromotionVote> {
    vec![
        HadrPromotionVote::grant(
            HadrNodeId::new(2),
            HadrEpoch::new(epoch),
            Lsn::new(safe_lsn),
        ),
        HadrPromotionVote::grant(
            HadrNodeId::new(3),
            HadrEpoch::new(epoch),
            Lsn::new(safe_lsn),
        ),
    ]
}

fn attempt(candidate_safe_lsn: u64, primary_durable_lsn: u64) -> PromotionAttempt {
    PromotionAttempt::new(
        HadrNodeId::new(2),
        HadrEpoch::new(3),
        Lsn::new(candidate_safe_lsn),
        Lsn::new(primary_durable_lsn),
        granting_votes(3, candidate_safe_lsn),
        fencing_at_epoch(3),
    )
}

fn cluster_audit_trace(
    surface: SurfaceScope,
    permission: Permission,
    outcome: SecurityAuditOutcome,
) -> SecurityAuditTrace {
    SecurityAuditTrace::new_with_policy_version(
        TraceId::new(16_001),
        surface,
        CertificateIdentity::new("fp-cluster-controller", "CN=hadr-controller", surface)
            .expect("certificate"),
        UserPrincipal::new("svc-hadr-controller", UserPrincipalKind::Service).expect("principal"),
        permission,
        outcome,
        SecurityPolicyVersionEvidence::new(
            16,
            "sha256:1616161616161616161616161616161616161616161616161616161616161616",
        )
        .expect("policy evidence"),
        "authorized HADR cluster operation",
    )
    .expect("security audit")
}

fn cluster_promotion_security() -> HadrClusterSecurityEvidence {
    HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        cluster_audit_trace(
            SurfaceScope::Cluster,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect("cluster promotion security")
}

#[test]
fn legacy_direct_promotion_requires_cluster_security_and_durable_audit() {
    let (_dir, store) = setup_three_node_store();
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);

    let err = boundary
        .promote(attempt(100, 100))
        .expect_err("legacy direct promotion must be fenced");

    assert!(err.message().contains("cluster security evidence"));
    assert!(err.message().contains("durable audit receipt"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
    assert!(!loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            ..
        } if *node_id == HadrNodeId::new(2) && *epoch == HadrEpoch::new(4)
    )));
}

#[test]
fn promotion_rejects_replica_behind_lsn() {
    let (_dir, store) = setup_three_node_store();
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);

    let err = boundary
        .promote_with_cluster_security(attempt(99, 100), cluster_promotion_security())
        .expect_err("behind replica must not be promoted");

    assert!(err.message().contains("behind primary durable LSN"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
}

#[test]
fn promotion_rejects_candidate_below_highest_granting_durable_lsn() {
    let (_dir, store) = setup_three_node_store();
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);
    let votes = vec![
        HadrPromotionVote::grant(HadrNodeId::new(2), HadrEpoch::new(3), Lsn::new(120)),
        HadrPromotionVote::grant(HadrNodeId::new(3), HadrEpoch::new(3), Lsn::new(150)),
    ];

    let err = boundary
        .promote_with_cluster_security(
            PromotionAttempt::new(
                HadrNodeId::new(2),
                HadrEpoch::new(3),
                Lsn::new(120),
                Lsn::new(100),
                votes,
                fencing_at_epoch(3),
            ),
            cluster_promotion_security(),
        )
        .expect_err("candidate must be at the highest granting durable LSN");

    assert!(err.message().contains("safe LSN is below"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
    assert!(!loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            ..
        } if *node_id == HadrNodeId::new(2) && *epoch == HadrEpoch::new(4)
    )));
}

#[test]
fn promotion_requires_quorum() {
    let (_dir, store) = setup_three_node_store();
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);
    let one_vote = vec![HadrPromotionVote::grant(
        HadrNodeId::new(2),
        HadrEpoch::new(3),
        Lsn::new(100),
    )];

    let err = boundary
        .promote_with_cluster_security(
            PromotionAttempt::new(
                HadrNodeId::new(2),
                HadrEpoch::new(3),
                Lsn::new(100),
                Lsn::new(100),
                one_vote,
                fencing_at_epoch(3),
            ),
            cluster_promotion_security(),
        )
        .expect_err("quorum is required");

    assert!(err.message().contains("quorum"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
}

#[test]
fn promotion_advances_epoch_once() {
    let (_dir, store) = setup_three_node_store();
    let audit = RecordingAudit::new(&store);
    let boundary = PromotionBoundary::new(&store, &audit);

    let commit = boundary
        .promote_with_cluster_security(attempt(100, 100), cluster_promotion_security())
        .expect("promote");

    assert_eq!(commit.snapshot.epoch(), HadrEpoch::new(4));
    assert_eq!(
        commit.snapshot.primary().expect("new primary").id,
        HadrNodeId::new(2)
    );
    let promotion_epoch_advances = commit
        .snapshot
        .records()
        .iter()
        .filter(|record| {
            matches!(
                record,
                HadrMembershipRecord::EpochAdvanced {
                    previous_epoch,
                    new_epoch,
                } if *previous_epoch == HadrEpoch::new(3) && *new_epoch == HadrEpoch::new(4)
            )
        })
        .count();
    assert_eq!(promotion_epoch_advances, 1);
    assert!(commit.snapshot.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            committed_safe_lsn,
        } if *node_id == HadrNodeId::new(2)
            && *epoch == HadrEpoch::new(4)
            && *committed_safe_lsn == Lsn::new(100)
    )));
}

struct RecordingAudit<'a> {
    store: &'a FileBackedHadrMembershipStore,
    primary_seen_during_audit: RefCell<Vec<Option<HadrNodeId>>>,
    markers: RefCell<Vec<HadrPromotionAuditMarker>>,
}

impl<'a> RecordingAudit<'a> {
    fn new(store: &'a FileBackedHadrMembershipStore) -> Self {
        Self {
            store,
            primary_seen_during_audit: RefCell::new(Vec::new()),
            markers: RefCell::new(Vec::new()),
        }
    }
}

impl HadrPromotionAuditLog for RecordingAudit<'_> {
    fn append_primary_promotion_marker(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> andromeda_core::AndromedaResult<()> {
        let primary = self
            .store
            .load()?
            .and_then(|snapshot| snapshot.primary().map(|node| node.id));
        self.primary_seen_during_audit.borrow_mut().push(primary);
        self.markers.borrow_mut().push(marker.clone());
        Ok(())
    }

    fn append_primary_promotion_marker_durably(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> andromeda_core::AndromedaResult<HadrPromotionAuditReceipt> {
        self.append_primary_promotion_marker(marker)?;
        HadrPromotionAuditReceipt::new(Lsn::new(marker.proposed_epoch.get()), [0x16; 32])
    }
}

#[test]
fn promotion_writes_audit_before_visible() {
    let (_dir, store) = setup_three_node_store();
    let audit = RecordingAudit::new(&store);
    let boundary = PromotionBoundary::new(&store, &audit);

    boundary
        .promote_with_cluster_security(attempt(100, 100), cluster_promotion_security())
        .expect("promote");

    assert_eq!(
        audit.primary_seen_during_audit.borrow().as_slice(),
        &[Some(HadrNodeId::new(1))]
    );
    assert_eq!(audit.markers.borrow()[0].candidate_id, HadrNodeId::new(2));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("new primary").id,
        HadrNodeId::new(2)
    );
}

#[test]
fn authorized_promotion_records_cluster_security_and_durable_audit_receipt_before_visible() {
    let (_dir, store) = setup_three_node_store();
    let audit = RecordingAudit::new(&store);
    let boundary = PromotionBoundary::new(&store, &audit);

    let commit = boundary
        .promote_with_cluster_security(attempt(100, 100), cluster_promotion_security())
        .expect("authorized promote");

    assert_eq!(
        audit.primary_seen_during_audit.borrow().as_slice(),
        &[Some(HadrNodeId::new(1))]
    );
    let marker = audit.markers.borrow()[0].clone();
    let security = marker
        .cluster_security
        .as_ref()
        .expect("cluster security marker");
    assert_eq!(security.operation(), HadrClusterOperation::PromotePrimary);
    assert_eq!(security.audit().permission, Permission::ClusterPromote);
    assert_eq!(security.audit().surface, SurfaceScope::Cluster);
    assert_eq!(security.audit().policy_version.policy_version, 16);
    assert_eq!(
        commit.audit_receipt.as_ref().expect("receipt").audit_lsn,
        Lsn::new(4)
    );
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("new primary").id,
        HadrNodeId::new(2)
    );
}

struct RejectingAudit;

impl HadrPromotionAuditLog for RejectingAudit {
    fn append_primary_promotion_marker(
        &self,
        _marker: &HadrPromotionAuditMarker,
    ) -> andromeda_core::AndromedaResult<()> {
        Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "promotion audit marker unavailable",
        ))
    }

    fn append_primary_promotion_marker_durably(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> andromeda_core::AndromedaResult<HadrPromotionAuditReceipt> {
        self.append_primary_promotion_marker(marker)?;
        unreachable!("rejecting audit log must fail before durable receipt")
    }
}

#[test]
fn promotion_requires_audit_marker_before_visible_primary() {
    let (_dir, store) = setup_three_node_store();
    let audit = RejectingAudit;
    let boundary = PromotionBoundary::new(&store, &audit);

    let error = boundary
        .promote_with_cluster_security(attempt(100, 100), cluster_promotion_security())
        .expect_err("audit failure blocks promotion");

    assert!(error.message().contains("audit marker unavailable"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
    assert!(!loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            ..
        } if *node_id == HadrNodeId::new(2) && *epoch == HadrEpoch::new(4)
    )));
}

#[test]
fn authorized_promotion_requires_durable_audit_receipt_before_visible_primary() {
    let (_dir, store) = setup_three_node_store();
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);

    let error = boundary
        .promote_with_cluster_security(attempt(100, 100), cluster_promotion_security())
        .expect_err("missing durable receipt blocks promotion");

    assert!(error.message().contains("durable audit receipt"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
    assert!(!loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            ..
        } if *node_id == HadrNodeId::new(2) && *epoch == HadrEpoch::new(4)
    )));
}

#[test]
fn cluster_promotion_security_rejects_non_cluster_surface() {
    let error = HadrClusterSecurityEvidence::new(
        HadrClusterOperation::PromotePrimary,
        cluster_audit_trace(
            SurfaceScope::Administration,
            Permission::ClusterPromote,
            SecurityAuditOutcome::Allowed,
        ),
    )
    .expect_err("promotion must require cluster surface");

    assert!(error.message().contains("Cluster surface"));
}
