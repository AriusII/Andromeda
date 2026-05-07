use crate::diagnostic_json::json_string;

use super::super::{demote, failover_prepare, output, promote, quorum, run_hadr_command, status};

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
fn hadr_promote_accepts_json_output() {
    let result = promote::run_hadr_promote(&[
        "2".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert!(result.is_ok());
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
fn hadr_json_string_escapes_diagnostic_fields() {
    assert_eq!(json_string("primary\trole"), "\"primary\\trole\"");
}

#[test]
fn hadr_signed_json_option_preserves_negative_lsn_distance() {
    assert_eq!(output::json_option_i64(Some(-1024)), "-1024");
    assert_eq!(output::json_option_i64(None), "null");
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
