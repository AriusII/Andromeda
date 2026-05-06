use super::*;
use crate::diagnostic_json::json_string;

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
    let result = promote::run_hadr_promote(&["2".to_string()]);
    assert!(result.is_ok());
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
    let result = demote::run_hadr_demote(&["--force".to_string()]);
    assert!(result.is_ok());
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
    let result = promote::run_hadr_promote(&["2".to_string(), "--json".to_string()]);
    assert!(result.is_ok());
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
