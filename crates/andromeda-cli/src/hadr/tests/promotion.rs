use std::fs;

use andromeda_storage::{
    HadrEpoch, HadrMembershipRecord, HadrMembershipStore, HadrNodeId, HadrNodeRole,
};

use super::super::promote;
use super::support::{
    RecordingPromotionAudit, assert_durable_primary, assert_primary,
    assert_primary_promoted_record, durable_store_path, load_membership_snapshot,
    load_membership_snapshot_from_store, open_membership_store, promote_membership_args,
    promotion_attempt, seed_three_node_membership_store,
};

#[test]
fn hadr_promote_requires_quorum() {
    let path = durable_store_path("promote-requires-quorum");
    seed_three_node_membership_store(&path);

    let result = promote::run_hadr_promote(&promote_membership_args(
        &path, None, "--apply", 100, 100, 1,
    ));

    let err = result.expect_err("promotion must require quorum");
    assert!(err.message().contains("quorum"));
    assert_durable_primary(&path, 1, "primary unchanged");
}

#[test]
fn hadr_promote_rejects_candidate_lsn_below_audit_lsn() {
    let path = durable_store_path("promote-rejects-stale-candidate");
    seed_three_node_membership_store(&path);

    let result =
        promote::run_hadr_promote(&promote_membership_args(&path, None, "--apply", 99, 100, 2));

    let err = result.expect_err("promotion must reject stale candidate LSN");
    assert!(err.message().contains("--candidate-lsn"));
    assert_durable_primary(&path, 1, "primary unchanged");
}

#[test]
fn hadr_promote_requires_primary_fencing_evidence() {
    let path = durable_store_path("promote-requires-primary-fence");
    let store = open_membership_store(&path);
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica 2");
    store
        .register_node(HadrNodeId::new(3), HadrNodeRole::Replica)
        .expect("register replica 3");

    let result = promote::run_hadr_promote(&promote_membership_args(
        &path, None, "--apply", 100, 100, 2,
    ));

    let err = result.expect_err("promotion must require primary fencing evidence");
    assert!(err.message().contains("fencing evidence"));
    let snapshot = store.load().expect("load").expect("snapshot");
    assert!(snapshot.primary().is_none());
}

#[test]
fn hadr_promote_dry_run_does_not_advance_epoch_or_write_audit_marker() {
    let path = durable_store_path("promote-dry-run");
    let audit_path = path.with_file_name("promotion-dry-run-audit.jsonl");
    seed_three_node_membership_store(&path);

    let result = promote::run_hadr_promote(&promote_membership_args(
        &path,
        Some(&audit_path),
        "--dry-run",
        100,
        100,
        2,
    ));

    assert!(result.is_ok());
    let snapshot = load_membership_snapshot(&path);
    assert_eq!(snapshot.epoch(), HadrEpoch::new(3));
    assert_primary(&snapshot, 1, "primary unchanged");
    assert!(!audit_path.exists());
}

#[test]
fn hadr_promote_apply_advances_epoch_once() {
    let path = durable_store_path("promote-advances-epoch-once");
    let audit_path = path.with_file_name("promotion-audit.jsonl");
    seed_three_node_membership_store(&path);

    let result = promote::run_hadr_promote(&promote_membership_args(
        &path,
        Some(&audit_path),
        "--apply",
        100,
        100,
        2,
    ));

    assert!(result.is_ok());
    let snapshot = load_membership_snapshot(&path);
    assert_eq!(snapshot.epoch(), HadrEpoch::new(4));
    assert_primary(&snapshot, 2, "new primary");
    let promotion_epoch_advances = snapshot
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
    assert_primary_promoted_record(&snapshot, 2, 4, 100);
    assert!(
        fs::read_to_string(audit_path)
            .expect("read audit log")
            .contains("\"event\":\"hadr.promotion.primary.marker\"")
    );
}

#[test]
fn hadr_promote_writes_audit_before_visible() {
    let path = durable_store_path("promote-audit-before-visible");
    seed_three_node_membership_store(&path);
    let store = open_membership_store(&path);
    let audit = RecordingPromotionAudit::new(&store);

    promote::apply_durable_promotion_with_audit(&store, &audit, promotion_attempt(100, 100))
        .expect("promote");

    assert_eq!(
        audit.primary_seen_during_audit.borrow().as_slice(),
        &[Some(HadrNodeId::new(1))]
    );
    assert_eq!(audit.markers.borrow()[0].candidate_id, HadrNodeId::new(2));
    let snapshot = load_membership_snapshot_from_store(&store);
    assert_primary(&snapshot, 2, "new primary");
}
