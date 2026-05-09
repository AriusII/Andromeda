use super::support::{
    assert_contains_all, assert_operator_boundary_json, assert_success, run_cli, stdout,
};

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
    assert_operator_boundary_json(&json, "andromeda.cli.backup.status.v1");
    assert_contains_all(
        &json,
        &[
            "\"backup_id\":100",
            "\"contract_preview\":true",
            "\"durable_backend\":false",
            "\"durable_state_loaded\":false",
            "\"runtime_mode\":\"contract_preview/static_scheduler\"",
            "\"requires_storage_scheduler\":true",
            "\"state\":\"contract_preview\"",
            "\"progress_percent\":0",
            "\"bytes_processed\":0",
            "\"estimated_total_bytes\":0",
            "\"start_time\":0",
            "\"elapsed_seconds\":0",
        ],
    );
}

#[test]
fn audit_inspect_json_is_operator_diagnostic_surface() {
    let output = run_cli([
        "audit",
        "inspect",
        "--trace-id",
        "42",
        "--principal",
        "user:ops",
        "--limit",
        "5",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_operator_boundary_json(&json, "andromeda.cli.audit.inspection.v1");
    assert_contains_all(
        &json,
        &[
            "\"diagnostic_only\":true",
            "\"surface\":\"administration\"",
            "\"required_permission\":\"inspect-plans\"",
            "\"audit_operation\":\"inspect-plans\"",
            "\"result\":null",
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
        "--dry-run",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_operator_boundary_json(&json, "andromeda.cli.backup.start.v1");
    assert!(json.contains("\"destination\":\"C:\\\\Backup\\\\\\\"hot\\\"\""));
}
