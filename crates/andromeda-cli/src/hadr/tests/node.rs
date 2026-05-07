use andromeda_storage::{HadrMembershipRecord, HadrMembershipStore, HadrNodeId, HadrNodeRole};

use super::super::node;
use super::support::{
    assert_membership_record, durable_store_path, load_membership_snapshot,
    node_membership_fencing_args, node_register_membership_args, open_membership_store,
    seed_membership_store,
};

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

    let result = node::run_hadr_node_register(&node_register_membership_args(4, &path, "--apply"));

    assert!(result.is_ok());
    let snapshot = load_membership_snapshot(&path);
    let node = snapshot
        .get(HadrNodeId::new(4))
        .expect("registered replica node");
    assert_eq!(node.role, HadrNodeRole::Replica);
}

#[test]
fn hadr_node_register_dry_run_does_not_mutate_durable_membership_store() {
    let path = durable_store_path("node-register-dry-run");

    let result =
        node::run_hadr_node_register(&node_register_membership_args(4, &path, "--dry-run"));

    assert!(result.is_ok());
    let store = open_membership_store(&path);
    assert!(store.load().expect("load membership").is_none());
}

#[test]
fn hadr_node_deregister_dry_run_does_not_mutate_durable_membership_store() {
    let path = durable_store_path("node-deregister-dry-run");
    seed_membership_store(&path);
    let before = load_membership_snapshot(&path);

    let result = node::run_hadr_node_deregister(&node_membership_fencing_args(
        2,
        &path,
        "ticket-123",
        "--dry-run",
    ));

    assert!(result.is_ok());
    let after = load_membership_snapshot(&path);
    assert_eq!(after, before);
}

#[test]
fn hadr_node_fence_dry_run_does_not_mutate_durable_membership_store() {
    let path = durable_store_path("node-fence-dry-run");
    seed_membership_store(&path);
    let before = load_membership_snapshot(&path);

    let result = node::run_hadr_node_fence(&node_membership_fencing_args(
        1,
        &path,
        "ticket-456",
        "--dry-run",
    ));

    assert!(result.is_ok());
    let after = load_membership_snapshot(&path);
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
    let snapshot = load_membership_snapshot(&store_path);
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

    let result = node::run_hadr_node_deregister(&node_membership_fencing_args(
        2,
        &path,
        "ticket-123",
        "--apply",
    ));

    assert!(result.is_ok());
    let snapshot = load_membership_snapshot(&path);
    assert!(!snapshot.contains(HadrNodeId::new(2)));
    assert_membership_record(&snapshot, |record| {
        matches!(
            record,
            HadrMembershipRecord::NodeDeregistered { node_id, .. }
                if *node_id == HadrNodeId::new(2)
        )
    });
}

#[test]
fn hadr_node_fence_apply_persists_membership() {
    let path = durable_store_path("node-fence");
    seed_membership_store(&path);

    let result = node::run_hadr_node_fence(&node_membership_fencing_args(
        1,
        &path,
        "ticket-456",
        "--apply",
    ));

    assert!(result.is_ok());
    let snapshot = load_membership_snapshot(&path);
    assert_eq!(
        snapshot.get(HadrNodeId::new(1)).expect("fenced node").role,
        HadrNodeRole::Replica
    );
    assert_membership_record(&snapshot, |record| {
        matches!(
            record,
            HadrMembershipRecord::NodeFenced { node_id, .. }
                if *node_id == HadrNodeId::new(1)
        )
    });
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
