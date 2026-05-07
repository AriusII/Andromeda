use super::*;
use crate::diagnostic_json::json_string;
use andromeda_storage::{
    FileBackedHadrMembershipStore, HadrEpoch, HadrFencingContext, HadrFencingToken,
    HadrMembershipRecord, HadrMembershipStore, HadrNodeId, HadrNodeRole, HadrPromotionAuditLog,
    HadrPromotionAuditMarker, HadrPromotionAuditReceipt, HadrPromotionVote, Lsn, PromotionAttempt,
};
use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn hadr_status_returns_ok() {
    let result = status::run_hadr_status(&[]);
    assert!(result.is_ok());
}

#[test]
fn hadr_status_accepts_json_output() {
    let result = status::run_hadr_status(&["--json".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_promote_requires_replica_id() {
    let result = promote::run_hadr_promote(&[]);
    assert!(result.is_err());
}

#[test]
fn hadr_promote_accepts_valid_replica_id() {
    let result = promote::run_hadr_promote(&["2".to_string(), "--dry-run".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_promote_requires_dry_run_without_backend() {
    let result = promote::run_hadr_promote(&["2".to_string()]);
    assert!(result.is_err());
}

#[test]
fn hadr_promote_rejects_invalid_replica_id() {
    let result = promote::run_hadr_promote(&["not_a_number".to_string()]);
    assert!(result.is_err());
}

#[test]
fn hadr_demote_without_force_uses_safe_dry_path() {
    let result = demote::run_hadr_demote(&[]);
    assert!(result.is_ok());
}

#[test]
fn hadr_demote_with_force() {
    let result = demote::run_hadr_demote(&["--force".to_string(), "--dry-run".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_demote_force_requires_dry_run_without_backend() {
    let result = demote::run_hadr_demote(&["--force".to_string()]);
    assert!(result.is_err());
}

#[test]
fn hadr_failover_prepare_returns_ok() {
    let result = failover_prepare::run_hadr_failover_prepare(&[]);
    assert!(result.is_ok());
}

#[test]
fn hadr_failover_prepare_accepts_json_output() {
    let result = failover_prepare::run_hadr_failover_prepare(&["--json".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_failover_prepare_with_witness_check() {
    let result = failover_prepare::run_hadr_failover_prepare(&["--witness-check".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_failover_prepare_json_with_witness_check() {
    let result = failover_prepare::run_hadr_failover_prepare(&[
        "--json".to_string(),
        "--witness-check".to_string(),
    ]);
    assert!(result.is_ok());
}

#[test]
fn hadr_quorum_returns_ok() {
    let result = quorum::run_hadr_quorum(&[]);
    assert!(result.is_ok());
}

#[test]
fn hadr_quorum_accepts_json_output() {
    let result = quorum::run_hadr_quorum(&["--json".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_status_reads_durable_membership_store() {
    let path = durable_store_path("status");
    seed_membership_store(&path);

    let result = status::run_hadr_status(&[
        "--membership-store".to_string(),
        path.display().to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
}

#[test]
fn hadr_quorum_reads_durable_membership_store() {
    let path = durable_store_path("quorum");
    seed_membership_store(&path);

    let result = quorum::run_hadr_quorum(&[
        "--membership-store".to_string(),
        path.display().to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
}

#[test]
fn hadr_node_list_reads_durable_membership_store() {
    let path = durable_store_path("node-list");
    seed_membership_store(&path);

    let result = node::run_hadr_node_list(&[
        "--membership-store".to_string(),
        path.display().to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
}

#[test]
fn hadr_node_status_reads_durable_membership_store() {
    let path = durable_store_path("node-status");
    seed_membership_store(&path);

    let result = node::run_hadr_node_status(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
}

#[test]
fn hadr_node_register_requires_dry_run() {
    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_register_dry_run_accepts_replica_role() {
    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_ok());
}

#[test]
fn hadr_node_register_apply_persists_membership() {
    let path = durable_store_path("node-register");

    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--apply".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store
        .load()
        .expect("load membership")
        .expect("membership snapshot");
    let node = snapshot
        .get(HadrNodeId::new(4))
        .expect("registered replica node");
    assert_eq!(node.role, HadrNodeRole::Replica);
}

#[test]
fn hadr_node_register_dry_run_does_not_mutate_durable_membership_store() {
    let path = durable_store_path("node-register-dry-run");

    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    assert!(store.load().expect("load membership").is_none());
}

#[test]
fn hadr_node_deregister_dry_run_does_not_mutate_durable_membership_store() {
    let path = durable_store_path("node-deregister-dry-run");
    seed_membership_store(&path);
    let before = FileBackedHadrMembershipStore::open(&path)
        .expect("open membership store")
        .load()
        .expect("load membership")
        .expect("snapshot before dry-run");

    let result = node::run_hadr_node_deregister(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--fencing-evidence".to_string(),
        "ticket-123".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let after = FileBackedHadrMembershipStore::open(&path)
        .expect("open membership store")
        .load()
        .expect("load membership")
        .expect("snapshot after dry-run");
    assert_eq!(after, before);
}

#[test]
fn hadr_node_fence_dry_run_does_not_mutate_durable_membership_store() {
    let path = durable_store_path("node-fence-dry-run");
    seed_membership_store(&path);
    let before = FileBackedHadrMembershipStore::open(&path)
        .expect("open membership store")
        .load()
        .expect("load membership")
        .expect("snapshot before dry-run");

    let result = node::run_hadr_node_fence(&[
        "1".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--fencing-evidence".to_string(),
        "ticket-456".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let after = FileBackedHadrMembershipStore::open(&path)
        .expect("open membership store")
        .load()
        .expect("load membership")
        .expect("snapshot after dry-run");
    assert_eq!(after, before);
}

#[test]
fn hadr_node_register_state_dir_uses_default_membership_file() {
    let path = durable_store_path("node-register-state-dir");
    let state_dir = path.parent().expect("state dir");

    let result = node::run_hadr_node_register(&[
        "5".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--state-dir".to_string(),
        state_dir.display().to_string(),
        "--apply".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store_path = state_dir.join("hadr-membership.bin");
    let store = FileBackedHadrMembershipStore::open(&store_path).expect("open membership store");
    let snapshot = store
        .load()
        .expect("load membership")
        .expect("membership snapshot");
    assert!(snapshot.contains(HadrNodeId::new(5)));
}

#[test]
fn hadr_node_register_rejects_zero_node_id() {
    let result = node::run_hadr_node_register(&[
        "0".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_register_rejects_primary_role() {
    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "primary".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_deregister_dry_run_requires_fencing_evidence() {
    let result = node::run_hadr_node_deregister(&["4".to_string(), "--dry-run".to_string()]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_deregister_dry_run_accepts_fencing_evidence() {
    let result = node::run_hadr_node_deregister(&[
        "4".to_string(),
        "--fencing-evidence".to_string(),
        "ticket-123".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_ok());
}

#[test]
fn hadr_node_deregister_apply_persists_membership() {
    let path = durable_store_path("node-deregister");
    seed_membership_store(&path);

    let result = node::run_hadr_node_deregister(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--fencing-evidence".to_string(),
        "ticket-123".to_string(),
        "--apply".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store.load().expect("load membership").expect("snapshot");
    assert!(!snapshot.contains(HadrNodeId::new(2)));
    assert!(snapshot.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::NodeDeregistered { node_id, .. }
            if *node_id == HadrNodeId::new(2)
    )));
}

#[test]
fn hadr_node_fence_apply_persists_membership() {
    let path = durable_store_path("node-fence");
    seed_membership_store(&path);

    let result = node::run_hadr_node_fence(&[
        "1".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--fencing-evidence".to_string(),
        "ticket-456".to_string(),
        "--apply".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store.load().expect("load membership").expect("snapshot");
    assert_eq!(
        snapshot.get(HadrNodeId::new(1)).expect("fenced node").role,
        HadrNodeRole::Replica
    );
    assert!(snapshot.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::NodeFenced { node_id, .. }
            if *node_id == HadrNodeId::new(1)
    )));
}

#[test]
fn hadr_node_list_is_read_only_contract_preview() {
    let result = node::run_hadr_node_list(&[]);
    assert!(result.is_ok());
}

#[test]
fn hadr_node_list_rejects_unknown_machine_output_format() {
    let result = node::run_hadr_node_list(&["--csv".to_string()]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_register_rejects_unknown_machine_output_format() {
    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--dry-run".to_string(),
        "--csv".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_register_rejects_missing_role_value() {
    let result = node::run_hadr_node_register(&[
        "4".to_string(),
        "--role".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn hadr_node_status_requires_node_id() {
    let result = node::run_hadr_node_status(&[]);
    assert!(result.is_err());
}

#[test]
fn hadr_promote_accepts_json_output() {
    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert!(result.is_ok());
}

#[test]
fn hadr_promote_requires_quorum() {
    let path = durable_store_path("promote-requires-quorum");
    seed_three_node_membership_store(&path);

    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--apply".to_string(),
        "--candidate-lsn".to_string(),
        "100".to_string(),
        "--audit-lsn".to_string(),
        "100".to_string(),
        "--commit-quorum".to_string(),
        "1".to_string(),
        "--json".to_string(),
    ]);

    let err = result.expect_err("promotion must require quorum");
    assert!(err.message().contains("quorum"));
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store.load().expect("load").expect("snapshot");
    assert_eq!(
        snapshot.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
}

#[test]
fn hadr_promote_rejects_candidate_lsn_below_audit_lsn() {
    let path = durable_store_path("promote-rejects-stale-candidate");
    seed_three_node_membership_store(&path);

    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--apply".to_string(),
        "--candidate-lsn".to_string(),
        "99".to_string(),
        "--audit-lsn".to_string(),
        "100".to_string(),
        "--commit-quorum".to_string(),
        "2".to_string(),
        "--json".to_string(),
    ]);

    let err = result.expect_err("promotion must reject stale candidate LSN");
    assert!(err.message().contains("--candidate-lsn"));
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store.load().expect("load").expect("snapshot");
    assert_eq!(
        snapshot.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
}

#[test]
fn hadr_promote_requires_primary_fencing_evidence() {
    let path = durable_store_path("promote-requires-primary-fence");
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica 2");
    store
        .register_node(HadrNodeId::new(3), HadrNodeRole::Replica)
        .expect("register replica 3");

    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--apply".to_string(),
        "--candidate-lsn".to_string(),
        "100".to_string(),
        "--audit-lsn".to_string(),
        "100".to_string(),
        "--commit-quorum".to_string(),
        "2".to_string(),
        "--json".to_string(),
    ]);

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

    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--promotion-audit-log".to_string(),
        audit_path.display().to_string(),
        "--dry-run".to_string(),
        "--candidate-lsn".to_string(),
        "100".to_string(),
        "--audit-lsn".to_string(),
        "100".to_string(),
        "--commit-quorum".to_string(),
        "2".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store.load().expect("load").expect("snapshot");
    assert_eq!(snapshot.epoch(), HadrEpoch::new(3));
    assert_eq!(
        snapshot.primary().expect("primary unchanged").id,
        HadrNodeId::new(1)
    );
    assert!(!audit_path.exists());
}

#[test]
fn hadr_promote_apply_advances_epoch_once() {
    let path = durable_store_path("promote-advances-epoch-once");
    let audit_path = path.with_file_name("promotion-audit.jsonl");
    seed_three_node_membership_store(&path);

    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--promotion-audit-log".to_string(),
        audit_path.display().to_string(),
        "--apply".to_string(),
        "--candidate-lsn".to_string(),
        "100".to_string(),
        "--audit-lsn".to_string(),
        "100".to_string(),
        "--commit-quorum".to_string(),
        "2".to_string(),
        "--json".to_string(),
    ]);

    assert!(result.is_ok());
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let snapshot = store.load().expect("load").expect("snapshot");
    assert_eq!(snapshot.epoch(), HadrEpoch::new(4));
    assert_eq!(
        snapshot.primary().expect("new primary").id,
        HadrNodeId::new(2)
    );
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
    assert!(snapshot.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            committed_safe_lsn,
        } if *node_id == HadrNodeId::new(2)
            && *epoch == HadrEpoch::new(4)
            && *committed_safe_lsn == Lsn::new(100)
    )));
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
    let store = FileBackedHadrMembershipStore::open(&path).expect("open membership store");
    let audit = RecordingPromotionAudit::new(&store);

    promote::apply_durable_promotion_with_audit(&store, &audit, promotion_attempt(100, 100))
        .expect("promote");

    assert_eq!(
        audit.primary_seen_during_audit.borrow().as_slice(),
        &[Some(HadrNodeId::new(1))]
    );
    assert_eq!(audit.markers.borrow()[0].candidate_id, HadrNodeId::new(2));
    let snapshot = store.load().expect("load").expect("snapshot");
    assert_eq!(
        snapshot.primary().expect("new primary").id,
        HadrNodeId::new(2)
    );
}

#[test]
fn hadr_json_string_escapes_diagnostic_fields() {
    assert_eq!(json_string("primary\trole"), "\"primary\\trole\"");
}

#[test]
fn hadr_command_unknown_subcommand() {
    let result = run_hadr_command(&["unknown".to_string()]);
    assert!(result.is_err());
}

#[test]
fn hadr_command_help() {
    let result = run_hadr_command(&["--help".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn hadr_command_no_args_shows_help() {
    let result = run_hadr_command(&[]);
    assert!(result.is_ok());
}

fn seed_membership_store(path: &PathBuf) {
    let store = FileBackedHadrMembershipStore::open(path).expect("open membership store");
    store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica");
}

fn seed_three_node_membership_store(path: &PathBuf) {
    seed_membership_store(path);
    let store = FileBackedHadrMembershipStore::open(path).expect("open membership store");
    store
        .register_node(HadrNodeId::new(3), HadrNodeRole::Replica)
        .expect("register replica 3");
}

fn promotion_attempt(candidate_safe_lsn: u64, primary_durable_lsn: u64) -> PromotionAttempt {
    PromotionAttempt::new(
        HadrNodeId::new(2),
        HadrEpoch::new(3),
        Lsn::new(candidate_safe_lsn),
        Lsn::new(primary_durable_lsn),
        vec![
            HadrPromotionVote::grant(
                HadrNodeId::new(2),
                HadrEpoch::new(3),
                Lsn::new(candidate_safe_lsn),
            ),
            HadrPromotionVote::grant(
                HadrNodeId::new(3),
                HadrEpoch::new(3),
                Lsn::new(candidate_safe_lsn),
            ),
        ],
        HadrFencingContext::with_active(
            HadrFencingToken::new(HadrNodeId::new(1), HadrEpoch::new(3)),
            HadrEpoch::new(3),
        ),
    )
}

struct RecordingPromotionAudit<'a> {
    store: &'a FileBackedHadrMembershipStore,
    primary_seen_during_audit: RefCell<Vec<Option<HadrNodeId>>>,
    markers: RefCell<Vec<HadrPromotionAuditMarker>>,
}

impl<'a> RecordingPromotionAudit<'a> {
    fn new(store: &'a FileBackedHadrMembershipStore) -> Self {
        Self {
            store,
            primary_seen_during_audit: RefCell::new(Vec::new()),
            markers: RefCell::new(Vec::new()),
        }
    }
}

impl HadrPromotionAuditLog for RecordingPromotionAudit<'_> {
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
        HadrPromotionAuditReceipt::new(marker.primary_durable_lsn, [0x16; 32])
    }
}

fn durable_store_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "andromeda-cli-hadr-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp HADR dir");
    dir.join("membership.bin")
}
