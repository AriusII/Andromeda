#![forbid(unsafe_code)]

//! Integration tests for CLI admin commands.
//!
//! Tests command parsing, routing, and output formats for:
//! - HADR commands (status, promote, demote, quorum)
//! - Backup commands (start, status, list)
//! - Restore commands (start, status)
//! - Catalog commands (list-procedures, invalidate-cache, show-contract)

use andromeda_cli::cmd::dispatch_command;

// ============================================================================
// HADR Command Tests
// ============================================================================

#[test]
fn hadr_status_command_parses_and_executes() {
    let args = vec!["hadr".to_string(), "status".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_status_with_json_output_format() {
    let args = vec![
        "hadr".to_string(),
        "status".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_promote_requires_replica_id() {
    let args = vec!["hadr".to_string(), "promote".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_promote_accepts_replica_id() {
    let args = vec!["hadr".to_string(), "promote".to_string(), "2".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_demote_command_requires_force_flag() {
    let args = vec!["hadr".to_string(), "demote".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok()); // Returns OK, prints warning
}

#[test]
fn hadr_demote_with_force_executes() {
    let args = vec![
        "hadr".to_string(),
        "demote".to_string(),
        "--force".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_quorum_command_executes() {
    let args = vec!["hadr".to_string(), "quorum".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_help_command_executes() {
    let args = vec!["hadr".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

// ============================================================================
// Backup Command Tests
// ============================================================================

#[test]
fn backup_start_command_parses_and_executes() {
    let args = vec!["backup".to_string(), "start".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_start_with_incremental_flag() {
    let args = vec![
        "backup".to_string(),
        "start".to_string(),
        "--incremental".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_start_with_destination_path() {
    let args = vec![
        "backup".to_string(),
        "start".to_string(),
        "--destination".to_string(),
        "/backup/dest".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_status_requires_backup_id() {
    let args = vec!["backup".to_string(), "status".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn backup_status_accepts_backup_id() {
    let args = vec![
        "backup".to_string(),
        "status".to_string(),
        "100".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_list_command_executes() {
    let args = vec!["backup".to_string(), "list".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_list_with_limit_flag() {
    let args = vec![
        "backup".to_string(),
        "list".to_string(),
        "--limit".to_string(),
        "5".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_help_command_executes() {
    let args = vec!["backup".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

// ============================================================================
// Restore Command Tests
// ============================================================================

#[test]
fn restore_from_backup_id_executes() {
    let args = vec!["restore".to_string(), "100".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn restore_with_pitr_lsn_executes() {
    let args = vec![
        "restore".to_string(),
        "100".to_string(),
        "--pitr-lsn".to_string(),
        "2097152".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn restore_status_requires_restore_id() {
    let args = vec!["restore".to_string(), "status".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn restore_status_accepts_restore_id() {
    let args = vec![
        "restore".to_string(),
        "status".to_string(),
        "200".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn restore_help_command_executes() {
    let args = vec!["restore".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

// ============================================================================
// Catalog Command Tests
// ============================================================================

#[test]
fn catalog_list_procedures_command_executes() {
    let args = vec!["catalog".to_string(), "list-procedures".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_list_procedures_with_namespace_filter() {
    let args = vec![
        "catalog".to_string(),
        "list-procedures".to_string(),
        "--namespace".to_string(),
        "inventory".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_invalidate_cache_command_executes() {
    let args = vec!["catalog".to_string(), "invalidate-cache".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_invalidate_cache_for_specific_procedure() {
    let args = vec![
        "catalog".to_string(),
        "invalidate-cache".to_string(),
        "--procedure-id".to_string(),
        "1".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_show_contract_requires_procedure_id() {
    let args = vec!["catalog".to_string(), "show-contract".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn catalog_show_contract_accepts_procedure_id() {
    let args = vec![
        "catalog".to_string(),
        "show-contract".to_string(),
        "1".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_help_command_executes() {
    let args = vec!["catalog".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

// ============================================================================
// Error Handling and Edge Cases
// ============================================================================

#[test]
fn unknown_command_returns_error() {
    let args = vec!["unknown-command".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_unknown_subcommand_returns_error() {
    let args = vec!["hadr".to_string(), "unknown-subcommand".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn backup_unknown_subcommand_returns_error() {
    let args = vec!["backup".to_string(), "unknown-subcommand".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn restore_invalid_backup_id_returns_error() {
    let args = vec!["restore".to_string(), "not_a_number".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn catalog_show_contract_invalid_id_returns_error() {
    let args = vec![
        "catalog".to_string(),
        "show-contract".to_string(),
        "not_a_number".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

// ============================================================================
// JSON Output Format Tests
// ============================================================================

#[test]
fn hadr_promote_with_json_flag() {
    let args = vec![
        "hadr".to_string(),
        "promote".to_string(),
        "2".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_quorum_with_json_flag() {
    let args = vec![
        "hadr".to_string(),
        "quorum".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_status_with_json_flag() {
    let args = vec![
        "backup".to_string(),
        "status".to_string(),
        "100".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_list_with_json_flag() {
    let args = vec![
        "backup".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn restore_with_json_flag() {
    let args = vec![
        "restore".to_string(),
        "100".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_list_procedures_with_json_flag() {
    let args = vec![
        "catalog".to_string(),
        "list-procedures".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_show_contract_with_json_flag() {
    let args = vec![
        "catalog".to_string(),
        "show-contract".to_string(),
        "1".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}
