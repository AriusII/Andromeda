#![forbid(unsafe_code)]

//! Integration tests for CLI admin commands.
//!
//! Tests command parsing, routing, and output policy for:
//! - HADR commands (status, promote, demote, quorum)
//! - Backup commands (start, status, list)
//! - Restore commands (start, status)
//! - Catalog commands (list-procedures, invalidate-cache, show-contract)

use andromeda_cli::cmd::dispatch_command;
use std::process::{Command, Output};

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
fn hadr_status_accepts_json_output() {
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
fn hadr_node_list_command_executes_as_contract_scaffold() {
    let args = vec!["hadr".to_string(), "node".to_string(), "list".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_node_status_requires_node_id() {
    let args = vec!["hadr".to_string(), "node".to_string(), "status".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_node_register_requires_dry_run() {
    let args = vec![
        "hadr".to_string(),
        "node".to_string(),
        "register".to_string(),
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_node_register_dry_run_accepts_replica_only() {
    let args = vec![
        "hadr".to_string(),
        "node".to_string(),
        "register".to_string(),
        "4".to_string(),
        "--role".to_string(),
        "replica".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_node_register_rejects_primary_role() {
    let args = vec![
        "hadr".to_string(),
        "node".to_string(),
        "register".to_string(),
        "4".to_string(),
        "--role".to_string(),
        "primary".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_node_deregister_dry_run_requires_fencing_evidence() {
    let args = vec![
        "hadr".to_string(),
        "node".to_string(),
        "deregister".to_string(),
        "4".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_node_deregister_dry_run_accepts_fencing_evidence() {
    let args = vec![
        "hadr".to_string(),
        "node".to_string(),
        "deregister".to_string(),
        "4".to_string(),
        "--fencing-evidence".to_string(),
        "operator-ticket-123".to_string(),
        "--dry-run".to_string(),
    ];
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
// Benchmark Command Tests
// ============================================================================

#[test]
fn benchmark_help_command_executes() {
    let args = vec!["benchmark".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn benchmark_workloads_command_executes() {
    let args = vec!["benchmark".to_string(), "workloads".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn benchmark_contract_accepts_diagnostic_json() {
    let args = vec![
        "benchmark".to_string(),
        "contract".to_string(),
        "--diagnostic-json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn benchmark_rejects_plain_json_alias() {
    let args = vec![
        "benchmark".to_string(),
        "contract".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn benchmark_run_rejects_unbounded_samples() {
    let args = vec![
        "benchmark".to_string(),
        "run".to_string(),
        "vertical-v0-smoke".to_string(),
        "--samples".to_string(),
        "101".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn benchmark_run_executes_bounded_smoke_runner() {
    let args = vec![
        "benchmark".to_string(),
        "run".to_string(),
        "vertical-v0-smoke".to_string(),
        "--duration-ms".to_string(),
        "1000".to_string(),
        "--samples".to_string(),
        "1".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn benchmark_run_diagnostic_json_is_diagnostic_only() {
    let output = run_cli([
        "benchmark",
        "run",
        "protocol-smoke-contract",
        "--duration-ms",
        "1000",
        "--samples",
        "5",
        "--diagnostic-json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.benchmark.run.v1\"",
            "\"diagnostic_only\":true",
            "\"runner\":\"deterministic-smoke\"",
            "\"workload_id\":\"protocol-smoke-contract\"",
            "\"budget_status\":\"passed\"",
        ],
    );
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
// JSON Output Policy Tests
// ============================================================================

#[test]
fn hadr_promote_accepts_json_output() {
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
fn hadr_quorum_accepts_json_output() {
    let args = vec![
        "hadr".to_string(),
        "quorum".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_status_accepts_json_output() {
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
fn backup_list_accepts_json_output() {
    let args = vec![
        "backup".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn restore_accepts_json_output() {
    let args = vec![
        "restore".to_string(),
        "100".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_list_procedures_accepts_json_output() {
    let args = vec![
        "catalog".to_string(),
        "list-procedures".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_show_contract_accepts_json_output() {
    let args = vec![
        "catalog".to_string(),
        "show-contract".to_string(),
        "1".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn admin_json_output_is_opt_in_and_has_stable_backup_fields() {
    let human = run_cli(["backup", "status", "100"]);
    assert_success(&human);
    let human_stdout = stdout(&human);
    assert!(human_stdout.contains("Backup Status Report"));
    assert!(!human_stdout.trim_start().starts_with('{'));

    let machine = run_cli(["backup", "status", "100", "--json"]);
    assert_success(&machine);
    let json = stdout(&machine);
    assert!(json.trim_start().starts_with('{'));
    assert_contains_all(
        &json,
        &[
            "\"backup_id\":100",
            "\"state\":\"running\"",
            "\"progress_percent\":65",
            "\"bytes_processed\":1073741824",
            "\"estimated_total_bytes\":1610612736",
            "\"start_time\":",
            "\"elapsed_seconds\":120",
        ],
    );
}

#[test]
fn admin_json_output_escapes_operator_supplied_strings() {
    let output = run_cli([
        "backup",
        "start",
        "--destination",
        "C:\\Backup\\\"hot\"",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert!(json.trim_start().starts_with('{'));
    assert!(json.contains("\"destination\":\"C:\\\\Backup\\\\\\\"hot\\\"\""));
}

#[test]
fn unsupported_machine_output_formats_are_rejected() {
    for args in [
        vec!["backup", "status", "100", "--csv"],
        vec!["restore", "status", "200", "--csv"],
        vec!["hadr", "status", "--csv"],
        vec!["hadr", "demote", "--csv"],
        vec!["catalog", "show-contract", "1", "--csv"],
    ] {
        let output = run_cli_vec(args);
        assert!(
            !output.status.success(),
            "unsupported machine output format should fail"
        );
    }
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn run_cli_vec(args: Vec<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_contains_all(text: &str, expected: &[&str]) {
    for item in expected {
        assert!(text.contains(item), "expected `{item}` in `{text}`");
    }
}
