use std::fs;

use andromeda_storage::Lsn;
use andromeda_storage::hadr::{
    FileBackedHadrMembershipStore, HadrEpoch, HadrMembershipRecord, HadrMembershipStore,
    HadrNodeId, HadrNodeRole,
};
use sha2::{Digest, Sha256};

fn store_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("hadr-membership.store")
}

fn rewrite_membership_checksum(bytes: &mut [u8]) {
    let checksum = Sha256::digest(&bytes[56..]);
    bytes[16..48].copy_from_slice(&checksum);
}

#[test]
fn register_node_persists_after_reopen() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open empty store");

    let written = store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    assert_eq!(written.epoch(), HadrEpoch::new(1));

    let reopened = FileBackedHadrMembershipStore::open(&path).expect("reopen store");
    let loaded = reopened.load().expect("load snapshot").expect("snapshot");

    assert_eq!(loaded.epoch(), HadrEpoch::new(1));
    assert_eq!(loaded.nodes().len(), 1);
    assert_eq!(loaded.primary().expect("primary").id, HadrNodeId::new(1));
}

#[test]
fn duplicate_node_registration_is_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FileBackedHadrMembershipStore::open(store_path(&dir)).expect("open store");

    store
        .register_node(HadrNodeId::new(7), HadrNodeRole::Replica)
        .expect("register replica");
    let error = store
        .register_node(HadrNodeId::new(7), HadrNodeRole::Candidate)
        .expect_err("duplicate id must fail");

    assert!(error.message().contains("already exists"));
    let loaded = store.load().expect("load snapshot").expect("snapshot");
    assert_eq!(loaded.epoch(), HadrEpoch::new(1));
    assert_eq!(loaded.nodes().len(), 1);
    assert_eq!(loaded.nodes()[0].role, HadrNodeRole::Replica);
}

#[test]
fn membership_role_updates_increment_epoch_monotonically() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FileBackedHadrMembershipStore::open(store_path(&dir)).expect("open store");

    let registered = store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica");
    let candidate = store
        .update_node_role(HadrNodeId::new(2), HadrNodeRole::Candidate)
        .expect("stage candidate");
    let promoted = store
        .update_node_role(HadrNodeId::new(2), HadrNodeRole::Primary)
        .expect("promote primary");

    assert_eq!(registered.epoch(), HadrEpoch::new(1));
    assert_eq!(candidate.epoch(), HadrEpoch::new(2));
    assert_eq!(promoted.epoch(), HadrEpoch::new(3));
    assert_eq!(
        promoted.get(HadrNodeId::new(2)).expect("node").role_epoch,
        HadrEpoch::new(3)
    );
}

#[test]
fn two_primaries_are_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FileBackedHadrMembershipStore::open(store_path(&dir)).expect("open store");

    store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    let error = store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Primary)
        .expect_err("second primary must fail");

    assert!(error.message().contains("two primaries"));
    let loaded = store.load().expect("load snapshot").expect("snapshot");
    assert_eq!(loaded.nodes().len(), 1);
    assert_eq!(loaded.primary().expect("primary").id, HadrNodeId::new(1));
}

#[test]
fn corrupted_membership_file_is_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open store");
    store
        .register_node(HadrNodeId::new(9), HadrNodeRole::Replica)
        .expect("register replica");

    let mut bytes = fs::read(&path).expect("read membership file");
    let last = bytes.last_mut().expect("non-empty membership file");
    *last ^= 0x55;
    fs::write(&path, bytes).expect("write corrupted file");

    let error = FileBackedHadrMembershipStore::open(&path).expect_err("corrupted file must fail");
    assert!(error.message().contains("checksum mismatch"));
}

#[test]
fn reordered_membership_record_file_is_rejected_even_with_valid_hash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = store_path(&dir);
    let store = FileBackedHadrMembershipStore::open(&path).expect("open store");
    store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica");

    let mut bytes = fs::read(&path).expect("read membership file");
    let first_record = 56 + 32 + 2 * 24;
    let second_record = first_record + 32;
    for offset in 0..32 {
        bytes.swap(first_record + offset, second_record + offset);
    }
    rewrite_membership_checksum(&mut bytes);
    fs::write(&path, bytes).expect("write reordered file");

    let error =
        FileBackedHadrMembershipStore::open(&path).expect_err("reordered records must fail");
    assert!(
        error
            .message()
            .contains("action record must follow its epoch advance")
    );
}

#[test]
fn membership_store_persists_lifecycle_records() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FileBackedHadrMembershipStore::open(store_path(&dir)).expect("open store");

    store
        .register_node(HadrNodeId::new(1), HadrNodeRole::Primary)
        .expect("register primary");
    store
        .register_node(HadrNodeId::new(2), HadrNodeRole::Replica)
        .expect("register replica");
    store
        .fence_node(HadrNodeId::new(1))
        .expect("fence old primary");
    store
        .promote_primary(HadrNodeId::new(2), HadrEpoch::new(4), Lsn::new(900))
        .expect("promote replica");
    store
        .deregister_node(HadrNodeId::new(1))
        .expect("deregister old primary");

    let reopened = FileBackedHadrMembershipStore::open(store_path(&dir)).expect("reopen store");
    let loaded = reopened.load().expect("load").expect("snapshot");

    assert!(loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::NodeRegistered {
            node_id,
            role: HadrNodeRole::Primary,
            epoch,
        } if *node_id == HadrNodeId::new(1) && *epoch == HadrEpoch::new(1)
    )));
    assert!(loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::NodeFenced {
            node_id,
            epoch,
        } if *node_id == HadrNodeId::new(1) && *epoch == HadrEpoch::new(3)
    )));
    assert!(loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::PrimaryPromoted {
            node_id,
            epoch,
            committed_safe_lsn,
        } if *node_id == HadrNodeId::new(2)
            && *epoch == HadrEpoch::new(4)
            && *committed_safe_lsn == Lsn::new(900)
    )));
    assert!(loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::NodeDeregistered {
            node_id,
            epoch,
        } if *node_id == HadrNodeId::new(1) && *epoch == HadrEpoch::new(5)
    )));
    assert!(loaded.records().iter().any(|record| matches!(
        record,
        HadrMembershipRecord::EpochAdvanced {
            previous_epoch,
            new_epoch,
        } if *previous_epoch == HadrEpoch::new(3) && *new_epoch == HadrEpoch::new(4)
    )));
}
