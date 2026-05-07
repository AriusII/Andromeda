use super::super::{node, quorum, status};
use super::support::{
    durable_store_path, membership_store_json_args, node_membership_store_json_args,
    seed_membership_store,
};

#[test]
fn hadr_status_reads_durable_membership_store() {
    let path = durable_store_path("status");
    seed_membership_store(&path);

    let result = status::run_hadr_status(&membership_store_json_args(&path));

    assert!(result.is_ok());
}

#[test]
fn hadr_quorum_reads_durable_membership_store() {
    let path = durable_store_path("quorum");
    seed_membership_store(&path);

    let result = quorum::run_hadr_quorum(&membership_store_json_args(&path));

    assert!(result.is_ok());
}

#[test]
fn hadr_node_list_reads_durable_membership_store() {
    let path = durable_store_path("node-list");
    seed_membership_store(&path);

    let result = node::run_hadr_node_list(&membership_store_json_args(&path));

    assert!(result.is_ok());
}

#[test]
fn hadr_node_status_reads_durable_membership_store() {
    let path = durable_store_path("node-status");
    seed_membership_store(&path);

    let result = node::run_hadr_node_status(&node_membership_store_json_args(2, &path));

    assert!(result.is_ok());
}
