use super::support::{assert_dispatch_error, dispatch_error_message, run_cli, run_cli_vec};

#[test]
fn hadr_promote_rejects_flag_as_lsn_value_without_echoing_values() {
    assert_eq!(
        dispatch_error_message(["hadr", "promote", "2", "--candidate-lsn", "--json"]),
        "--candidate-lsn requires an unsigned integer"
    );
    assert!(
        !dispatch_error_message(["hadr", "promote", "2", "--token=super-secret"])
            .contains("super-secret")
    );
}

#[test]
fn backup_numeric_options_reject_following_flags_without_echoing_values() {
    assert_eq!(
        dispatch_error_message(["backup", "start", "--backup-id", "--json"]),
        "--backup-id requires a numeric argument"
    );
    assert_eq!(
        dispatch_error_message(["backup", "list", "--limit", "--json"]),
        "--limit requires a numeric argument"
    );
    assert!(
        !dispatch_error_message(["backup", "list", "--token=super-secret"])
            .contains("super-secret")
    );
}

#[test]
fn catalog_parser_errors_do_not_echo_values() {
    assert!(
        !dispatch_error_message(["catalog", "show-contract", "1", "--token=super-secret"])
            .contains("super-secret")
    );
    assert_eq!(
        dispatch_error_message(["catalog", "invalidate-cache", "--procedure-id", "--json"]),
        "--procedure-id requires a procedure ID argument"
    );
}

#[test]
fn benchmark_rejects_plain_json_alias() {
    assert_dispatch_error(["benchmark", "contract", "--json"]);
}

#[test]
fn benchmark_run_rejects_unbounded_samples() {
    assert_dispatch_error(["benchmark", "run", "vertical-v0-smoke", "--samples", "101"]);
}

#[test]
fn benchmark_run_rejects_zero_temp_budget() {
    assert_dispatch_error([
        "benchmark",
        "run",
        "vertical-v0-smoke",
        "--temp-budget-bytes",
        "0",
    ]);
}

#[test]
fn unknown_command_returns_error() {
    assert_dispatch_error(["unknown-command"]);
}

#[test]
fn hadr_unknown_subcommand_returns_error() {
    assert_dispatch_error(["hadr", "unknown-subcommand"]);
}

#[test]
fn backup_unknown_subcommand_returns_error() {
    assert_dispatch_error(["backup", "unknown-subcommand"]);
}

#[test]
fn restore_invalid_backup_id_returns_error() {
    assert_dispatch_error(["restore", "not_a_number"]);
}

#[test]
fn catalog_show_contract_invalid_id_returns_error() {
    assert_dispatch_error(["catalog", "show-contract", "not_a_number"]);
}

#[test]
fn backup_start_requires_dry_run_without_scheduler() {
    let output = run_cli(["backup", "start", "--json"]);
    assert!(
        !output.status.success(),
        "backup start without --dry-run should fail"
    );
}

#[test]
fn backup_list_rejects_zero_limit() {
    let output = run_cli(["backup", "list", "--limit", "0"]);
    assert!(
        !output.status.success(),
        "backup list --limit 0 should fail"
    );
}

#[test]
fn restore_lsn_options_reject_following_flags_without_echoing_values() {
    assert_eq!(
        dispatch_error_message(["restore", "100", "--pitr-lsn", "--json"]),
        "--pitr-lsn requires an LSN value"
    );
    assert_eq!(
        dispatch_error_message(["restore", "verify", "100", "--pitr-lsn", "--json"]),
        "--pitr-lsn requires an LSN value"
    );
    assert!(
        !dispatch_error_message(["restore", "100", "--pitr-policy", "super-secret"])
            .contains("super-secret")
    );
}

#[test]
fn operator_commands_reject_application_procedure_and_sql_surface_options() {
    for args in [
        vec!["audit", "inspect", "--sql", "select * from audit"],
        vec!["audit", "inspect", "--procedure-id", "1"],
        vec!["backup", "start", "--procedure-id", "1", "--dry-run"],
        vec![
            "backup",
            "start",
            "--sql",
            "select * from backup",
            "--dry-run",
        ],
        vec![
            "restore",
            "100",
            "--artifact",
            "target/backup",
            "--pitr-lsn",
            "1500",
            "--procedure-id",
            "1",
            "--dry-run",
        ],
        vec![
            "restore",
            "100",
            "--artifact",
            "target/backup",
            "--pitr-lsn",
            "1500",
            "--sql",
            "select * from restore",
            "--dry-run",
        ],
    ] {
        let output = run_cli_vec(args);
        assert!(
            !output.status.success(),
            "operator command accepted application procedure or SQL surface option"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.to_ascii_lowercase().contains("select *"),
            "parser error echoed ad hoc SQL text: {stderr}"
        );
    }
}

#[test]
fn unsupported_machine_output_formats_are_rejected() {
    for args in [
        vec!["backup", "status", "100", "--csv"],
        vec!["restore", "status", "200", "--csv"],
        vec!["hadr", "status", "--csv"],
        vec!["hadr", "demote", "--csv"],
        vec!["hadr", "node", "list", "--csv"],
        vec!["hadr", "node", "status", "4", "--csv"],
        vec![
            "hadr",
            "node",
            "register",
            "4",
            "--role",
            "replica",
            "--dry-run",
            "--csv",
        ],
        vec!["catalog", "show-contract", "1", "--csv"],
        vec![
            "catalog",
            "resolve-manifest",
            "--procedure-id",
            "1",
            "--csv",
        ],
    ] {
        let output = run_cli_vec(args);
        assert!(
            !output.status.success(),
            "unsupported machine output format should fail"
        );
    }
}
