use std::cell::RefCell;

use andromeda_core::{AndromedaError, AndromedaErrorKind};
use andromeda_storage::{
    Lsn,
    hadr::{
        FileBackedHadrMembershipStore, HadrEpoch, HadrFencingContext, HadrFencingToken,
        HadrMembershipRecord, HadrMembershipStore, HadrNodeId, HadrNodeRole, HadrPromotionAuditLog,
        HadrPromotionAuditMarker, HadrPromotionVote, NoopPromotionAuditLog, PromotionAttempt,
        PromotionBoundary,
    },
};

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

#[test]
fn promotion_rejects_replica_behind_lsn() {
    let (_dir, store) = setup_three_node_store();
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);

    let err = boundary
        .promote(attempt(99, 100))
        .expect_err("behind replica must not be promoted");

    assert!(err.message().contains("behind primary durable LSN"));
    let loaded = store.load().expect("load").expect("snapshot");
    assert_eq!(
        loaded.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
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
        .promote(PromotionAttempt::new(
            HadrNodeId::new(2),
            HadrEpoch::new(3),
            Lsn::new(100),
            Lsn::new(100),
            one_vote,
            fencing_at_epoch(3),
        ))
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
    let audit = NoopPromotionAuditLog;
    let boundary = PromotionBoundary::new(&store, &audit);

    let commit = boundary.promote(attempt(100, 100)).expect("promote");

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
}

#[test]
fn promotion_writes_audit_before_visible() {
    let (_dir, store) = setup_three_node_store();
    let audit = RecordingAudit::new(&store);
    let boundary = PromotionBoundary::new(&store, &audit);

    boundary.promote(attempt(100, 100)).expect("promote");

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
}

#[test]
fn promotion_requires_audit_marker_before_visible_primary() {
    let (_dir, store) = setup_three_node_store();
    let audit = RejectingAudit;
    let boundary = PromotionBoundary::new(&store, &audit);

    let error = boundary
        .promote(attempt(100, 100))
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
