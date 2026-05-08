use andromeda_storage::{
    FileBackedHadrMembershipStore, HadrEpoch, HadrFencingContext, HadrFencingToken,
    HadrMembershipRecord, HadrMembershipSnapshot, HadrMembershipStore, HadrNodeId, HadrNodeRole,
    HadrPromotionAuditLog, HadrPromotionAuditMarker, HadrPromotionAuditReceipt, HadrPromotionVote,
    Lsn, PromotionAttempt,
};
use andromeda_test_support::workspace::unique_temp_dir_path;
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn membership_store_json_args(path: &Path) -> Vec<String> {
    vec![
        "--membership-store".to_string(),
        path.display().to_string(),
        "--json".to_string(),
    ]
}

pub(super) fn node_membership_store_json_args(node_id: u64, path: &Path) -> Vec<String> {
    let mut args = vec![node_id.to_string()];
    args.extend(membership_store_json_args(path));
    args
}

pub(super) fn node_register_membership_args(node_id: u64, path: &Path, mode: &str) -> Vec<String> {
    vec![
        node_id.to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        mode.to_string(),
        "--json".to_string(),
    ]
}

pub(super) fn node_membership_fencing_args(
    node_id: u64,
    path: &Path,
    fencing_evidence: &str,
    mode: &str,
) -> Vec<String> {
    vec![
        node_id.to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
        "--fencing-evidence".to_string(),
        fencing_evidence.to_string(),
        mode.to_string(),
        "--json".to_string(),
    ]
}

pub(super) fn promote_membership_args(
    path: &Path,
    audit_path: Option<&Path>,
    mode: &str,
    candidate_lsn: u64,
    audit_lsn: u64,
    commit_quorum: u64,
) -> Vec<String> {
    let mut args = vec![
        "2".to_string(),
        "--membership-store".to_string(),
        path.display().to_string(),
    ];
    if let Some(audit_path) = audit_path {
        args.extend([
            "--promotion-audit-log".to_string(),
            audit_path.display().to_string(),
        ]);
    }
    args.extend([
        mode.to_string(),
        "--candidate-lsn".to_string(),
        candidate_lsn.to_string(),
        "--audit-lsn".to_string(),
        audit_lsn.to_string(),
        "--commit-quorum".to_string(),
        commit_quorum.to_string(),
        "--json".to_string(),
    ]);
    args
}

pub(super) fn open_membership_store(path: &Path) -> FileBackedHadrMembershipStore {
    FileBackedHadrMembershipStore::open(path).expect("open membership store")
}

pub(super) fn load_membership_snapshot(path: &Path) -> HadrMembershipSnapshot {
    let store = open_membership_store(path);
    load_membership_snapshot_from_store(&store)
}

pub(super) fn load_membership_snapshot_from_store(
    store: &FileBackedHadrMembershipStore,
) -> HadrMembershipSnapshot {
    store.load().expect("load membership").expect("snapshot")
}

pub(super) fn assert_durable_primary(path: &Path, node_id: u64, context: &str) {
    let snapshot = load_membership_snapshot(path);
    assert_primary(&snapshot, node_id, context);
}

pub(super) fn assert_primary(snapshot: &HadrMembershipSnapshot, node_id: u64, context: &str) {
    assert_eq!(
        snapshot.primary().expect(context).id,
        HadrNodeId::new(node_id)
    );
}

pub(super) fn assert_membership_record(
    snapshot: &HadrMembershipSnapshot,
    matches_record: impl Fn(&HadrMembershipRecord) -> bool,
) {
    assert!(snapshot.records().iter().any(matches_record));
}

pub(super) fn assert_primary_promoted_record(
    snapshot: &HadrMembershipSnapshot,
    node_id: u64,
    epoch: u64,
    committed_safe_lsn: u64,
) {
    assert_membership_record(snapshot, |record| {
        matches!(
            record,
            HadrMembershipRecord::PrimaryPromoted {
                node_id: promoted_node_id,
                epoch: promoted_epoch,
                committed_safe_lsn: promoted_safe_lsn,
            } if *promoted_node_id == HadrNodeId::new(node_id)
                && *promoted_epoch == HadrEpoch::new(epoch)
                && *promoted_safe_lsn == Lsn::new(committed_safe_lsn)
        )
    });
}

pub(super) fn seed_membership_store(path: &Path) {
    let store = open_membership_store(path);
    store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica");
}

pub(super) fn seed_three_node_membership_store(path: &Path) {
    seed_membership_store(path);
    let store = open_membership_store(path);
    store
        .register_node(HadrNodeId::new(3), HadrNodeRole::Replica)
        .expect("register replica 3");
}

pub(super) fn promotion_attempt(
    candidate_safe_lsn: u64,
    primary_durable_lsn: u64,
) -> PromotionAttempt {
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

pub(super) struct RecordingPromotionAudit<'a> {
    store: &'a FileBackedHadrMembershipStore,
    pub(super) primary_seen_during_audit: RefCell<Vec<Option<HadrNodeId>>>,
    pub(super) markers: RefCell<Vec<HadrPromotionAuditMarker>>,
}

impl<'a> RecordingPromotionAudit<'a> {
    pub(super) fn new(store: &'a FileBackedHadrMembershipStore) -> Self {
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

pub(super) fn durable_store_path(name: &str) -> PathBuf {
    let dir = unique_temp_dir_path(&format!("andromeda-cli-hadr-{name}"));
    fs::create_dir_all(&dir).expect("create temp HADR dir");
    dir.join("membership.bin")
}
