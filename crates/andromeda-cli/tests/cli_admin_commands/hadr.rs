use super::support::{
    assert_contains_all, assert_dispatch_error, assert_dispatch_success, assert_success,
    run_cli_vec, stdout,
};

#[test]
fn hadr_status_command_parses_and_executes() {
    assert_dispatch_success(["hadr", "status"]);
}

#[test]
fn hadr_status_accepts_json_output() {
    assert_dispatch_success(["hadr", "status", "--json"]);
}

#[test]
fn hadr_promote_requires_replica_id() {
    assert_dispatch_error(["hadr", "promote"]);
}

#[test]
fn hadr_promote_accepts_replica_id() {
    assert_dispatch_success(["hadr", "promote", "2", "--dry-run"]);
}

#[test]
fn hadr_promote_requires_dry_run_without_backend() {
    assert_dispatch_error(["hadr", "promote", "2"]);
}

#[test]
fn hadr_demote_command_requires_force_flag() {
    assert_dispatch_success(["hadr", "demote"]);
}

#[test]
fn hadr_demote_with_force_executes() {
    assert_dispatch_success(["hadr", "demote", "--force", "--dry-run"]);
}

#[test]
fn hadr_demote_force_requires_dry_run_without_backend() {
    assert_dispatch_error(["hadr", "demote", "--force"]);
}

#[test]
fn hadr_quorum_command_executes() {
    assert_dispatch_success(["hadr", "quorum"]);
}

#[test]
fn hadr_node_list_command_executes_as_contract_preview() {
    assert_dispatch_success(["hadr", "node", "list"]);
}

#[test]
fn hadr_node_status_requires_node_id() {
    assert_dispatch_error(["hadr", "node", "status"]);
}

#[test]
fn hadr_node_register_requires_dry_run() {
    assert_dispatch_error(["hadr", "node", "register", "4", "--role", "replica"]);
}

#[test]
fn hadr_node_register_dry_run_accepts_replica_only() {
    assert_dispatch_success([
        "hadr",
        "node",
        "register",
        "4",
        "--role",
        "replica",
        "--dry-run",
    ]);
}

#[test]
fn hadr_node_register_rejects_primary_role() {
    assert_dispatch_error([
        "hadr",
        "node",
        "register",
        "4",
        "--role",
        "primary",
        "--dry-run",
    ]);
}

#[test]
fn hadr_node_deregister_dry_run_requires_fencing_evidence() {
    assert_dispatch_error(["hadr", "node", "deregister", "4", "--dry-run"]);
}

#[test]
fn hadr_node_deregister_dry_run_accepts_fencing_evidence() {
    assert_dispatch_success([
        "hadr",
        "node",
        "deregister",
        "4",
        "--fencing-evidence",
        "operator-ticket-123",
        "--dry-run",
    ]);
}

#[test]
fn hadr_failover_prepare_command_executes() {
    assert_dispatch_success(["hadr", "failover-prepare"]);
}

#[test]
fn hadr_failover_prepare_accepts_json_output() {
    assert_dispatch_success(["hadr", "failover-prepare", "--json"]);
}

#[test]
fn hadr_failover_prepare_with_witness_check() {
    assert_dispatch_success(["hadr", "failover-prepare", "--witness-check"]);
}

#[test]
fn hadr_failover_prepare_json_with_witness_check() {
    assert_dispatch_success(["hadr", "failover-prepare", "--json", "--witness-check"]);
}

#[test]
fn hadr_help_command_executes() {
    assert_dispatch_success(["hadr", "--help"]);
}

#[test]
fn hadr_promote_accepts_json_output() {
    assert_dispatch_success(["hadr", "promote", "2", "--dry-run", "--json"]);
}

#[test]
fn hadr_quorum_accepts_json_output() {
    assert_dispatch_success(["hadr", "quorum", "--json"]);
}

#[test]
fn hadr_contract_preview_outputs_are_explicit() {
    for args in [
        vec!["hadr", "status", "--json"],
        vec!["hadr", "demote", "--json"],
        vec!["hadr", "quorum", "--json"],
        vec!["hadr", "failover-prepare", "--json"],
        vec!["hadr", "node", "list", "--json"],
        vec!["hadr", "node", "status", "4", "--json"],
    ] {
        let output = run_cli_vec(args);
        assert_success(&output);
        let json = stdout(&output);
        assert_contains_all(&json, &["\"contract_preview\":true"]);
    }
}

#[test]
fn hadr_mutating_dummy_commands_report_dry_run_preview() {
    for args in [
        vec!["hadr", "promote", "2", "--dry-run", "--json"],
        vec!["hadr", "demote", "--force", "--dry-run", "--json"],
        vec![
            "hadr",
            "node",
            "register",
            "4",
            "--role",
            "replica",
            "--dry-run",
            "--json",
        ],
        vec![
            "hadr",
            "node",
            "deregister",
            "4",
            "--fencing-evidence",
            "operator-ticket-123",
            "--dry-run",
            "--json",
        ],
    ] {
        let output = run_cli_vec(args);
        assert_success(&output);
        let json = stdout(&output);
        assert_contains_all(
            &json,
            &[
                "\"contract_preview\":true",
                "\"dry_run\":true",
                "\"would_apply\":false",
            ],
        );
    }
}
