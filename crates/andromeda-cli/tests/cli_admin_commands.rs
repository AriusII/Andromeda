#![forbid(unsafe_code)]

use andromeda_cli::cmd::dispatch_command;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

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
    let args = vec![
        "hadr".to_string(),
        "promote".to_string(),
        "2".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_promote_rejects_flag_as_lsn_value_without_echoing_values() {
    let args = vec![
        "hadr".to_string(),
        "promote".to_string(),
        "2".to_string(),
        "--candidate-lsn".to_string(),
        "--json".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(
        err.message(),
        "--candidate-lsn requires an unsigned integer"
    );

    let args = vec![
        "hadr".to_string(),
        "promote".to_string(),
        "2".to_string(),
        "--token=super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));
}

#[test]
fn hadr_promote_requires_dry_run_without_backend() {
    let args = vec!["hadr".to_string(), "promote".to_string(), "2".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_demote_command_requires_force_flag() {
    let args = vec!["hadr".to_string(), "demote".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_demote_with_force_executes() {
    let args = vec![
        "hadr".to_string(),
        "demote".to_string(),
        "--force".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_demote_force_requires_dry_run_without_backend() {
    let args = vec![
        "hadr".to_string(),
        "demote".to_string(),
        "--force".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn hadr_quorum_command_executes() {
    let args = vec!["hadr".to_string(), "quorum".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_node_list_command_executes_as_contract_preview() {
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
fn hadr_failover_prepare_command_executes() {
    let args = vec!["hadr".to_string(), "failover-prepare".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_failover_prepare_accepts_json_output() {
    let args = vec![
        "hadr".to_string(),
        "failover-prepare".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_failover_prepare_with_witness_check() {
    let args = vec![
        "hadr".to_string(),
        "failover-prepare".to_string(),
        "--witness-check".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn hadr_failover_prepare_json_with_witness_check() {
    let args = vec![
        "hadr".to_string(),
        "failover-prepare".to_string(),
        "--json".to_string(),
        "--witness-check".to_string(),
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

#[test]
fn backup_start_command_parses_and_executes() {
    let args = vec![
        "backup".to_string(),
        "start".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_start_with_incremental_flag() {
    let args = vec![
        "backup".to_string(),
        "start".to_string(),
        "--incremental".to_string(),
        "--dry-run".to_string(),
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
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn backup_numeric_options_reject_following_flags_without_echoing_values() {
    let args = vec![
        "backup".to_string(),
        "start".to_string(),
        "--backup-id".to_string(),
        "--json".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(err.message(), "--backup-id requires a numeric argument");

    let args = vec![
        "backup".to_string(),
        "list".to_string(),
        "--limit".to_string(),
        "--json".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(err.message(), "--limit requires a numeric argument");

    let args = vec![
        "backup".to_string(),
        "list".to_string(),
        "--token=super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));
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

#[test]
fn restore_from_backup_id_executes() {
    let artifact_dir = create_cli_backup_artifact("restore-from-backup-id", 8101);
    let args = vec![
        "restore".to_string(),
        "8101".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-policy".to_string(),
        "latest".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
    let _ = fs::remove_dir_all(artifact_dir);
}

#[test]
fn restore_with_pitr_lsn_executes() {
    let artifact_dir = create_cli_backup_artifact("restore-with-pitr-lsn", 8102);
    let args = vec![
        "restore".to_string(),
        "8102".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-lsn".to_string(),
        "1500".to_string(),
        "--dry-run".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
    let _ = fs::remove_dir_all(artifact_dir);
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
fn catalog_parser_errors_do_not_echo_values() {
    let args = vec![
        "catalog".to_string(),
        "show-contract".to_string(),
        "1".to_string(),
        "--token=super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));

    let args = vec![
        "catalog".to_string(),
        "invalidate-cache".to_string(),
        "--procedure-id".to_string(),
        "--json".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(
        err.message(),
        "--procedure-id requires a procedure ID argument"
    );
}

#[test]
fn catalog_resolve_manifest_requires_selector() {
    let args = vec!["catalog".to_string(), "resolve-manifest".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn catalog_resolve_manifest_accepts_procedure_id() {
    let args = vec![
        "catalog".to_string(),
        "resolve-manifest".to_string(),
        "--procedure-id".to_string(),
        "1".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn catalog_resolve_manifest_accepts_qualified_name() {
    let args = vec![
        "catalog".to_string(),
        "resolve-manifest".to_string(),
        "--qualified-name".to_string(),
        "Inventory.ReserveStock".to_string(),
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
fn benchmark_run_rejects_zero_temp_budget() {
    let args = vec![
        "benchmark".to_string(),
        "run".to_string(),
        "vertical-v0-smoke".to_string(),
        "--temp-budget-bytes".to_string(),
        "0".to_string(),
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

#[test]
fn hadr_promote_accepts_json_output() {
    let args = vec![
        "hadr".to_string(),
        "promote".to_string(),
        "2".to_string(),
        "--dry-run".to_string(),
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
    let artifact_dir = create_cli_backup_artifact("restore-json-output", 8103);
    let args = vec![
        "restore".to_string(),
        "8103".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-lsn".to_string(),
        "1500".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
    let _ = fs::remove_dir_all(artifact_dir);
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
fn catalog_preview_json_outputs_are_explicit() {
    for args in [
        vec!["catalog", "list-procedures", "--json"],
        vec!["catalog", "show-contract", "1", "--json"],
        vec!["catalog", "invalidate-cache", "--json"],
        vec![
            "catalog",
            "resolve-manifest",
            "--qualified-name",
            "Inventory.ReserveStock",
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
                "\"mode\":\"contract_preview/mock_ephemeral\"",
                "\"runtime\":\"mock_ephemeral\"",
                "\"durable_catalog_state\":false",
                "\"catalog_runtime_queried\":false",
                "no CatalogServerRuntime durable source was queried",
            ],
        );
    }
}

#[test]
fn catalog_invalidate_cache_json_does_not_claim_runtime_mutation() {
    let output = run_cli([
        "catalog",
        "invalidate-cache",
        "--procedure-id",
        "1",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.catalog.invalidate-cache.v1\"",
            "\"cache_invalidated\":false",
            "\"entries_cleared\":0",
            "no CatalogServerRuntime plan cache was invalidated",
        ],
    );
}

#[test]
fn catalog_resolve_manifest_json_is_preview_only() {
    let output = run_cli([
        "catalog",
        "resolve-manifest",
        "--procedure-id",
        "1",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.catalog.resolve-manifest.v1\"",
            "\"resolution_status\":\"preview_resolved\"",
            "\"procedure_id\":1",
            "\"contract_preview\":true",
            "\"durable_catalog_state\":false",
        ],
    );
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
    assert!(json.trim_start().starts_with('{'));
    assert!(json.contains("\"destination\":\"C:\\\\Backup\\\\\\\"hot\\\"\""));
}

#[test]
fn backup_dry_run_json_is_contract_preview_only() {
    let output = run_cli([
        "backup",
        "start",
        "--incremental",
        "--destination",
        "target/backup-preview",
        "--dry-run",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.backup.start.v1\"",
            "\"contract_preview\":true",
            "\"durable_backend\":false",
            "\"requires_storage_scheduler\":true",
            "\"dry_run\":true",
            "\"would_start\":false",
            "\"backup_id\":null",
            "\"backup_type\":\"incremental\"",
            "no durable backup was scheduled",
        ],
    );
}

#[test]
fn backup_start_creates_execution_plan() {
    let artifact_dir = temp_artifact_dir("backup-start-plan");
    let output = run_cli_owned(vec![
        "backup".to_string(),
        "start".to_string(),
        "--runtime".to_string(),
        "--artifact-dir".to_string(),
        artifact_dir.display().to_string(),
        "--backup-id".to_string(),
        "8201".to_string(),
        "--json".to_string(),
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.backup.start.v1\"",
            "\"contract_preview\":false",
            "\"durable_backend\":true",
            "\"requires_storage_scheduler\":false",
            "\"backup_id\":8201",
            "\"base_lsn\":1001",
            "\"end_lsn\":2000",
            "file-backed backup execution plan",
        ],
    );
    assert!(backup_manifest_path(&artifact_dir, 8201).is_file());
    let _ = fs::remove_dir_all(artifact_dir);
}

#[test]
fn backup_runtime_commands_use_file_backed_artifacts() {
    let artifact_dir = create_cli_backup_artifact("backup-runtime-commands", 8206);

    for args in [
        vec![
            "backup".to_string(),
            "status".to_string(),
            "8206".to_string(),
            "--runtime".to_string(),
            "--artifact-dir".to_string(),
            artifact_dir.display().to_string(),
            "--json".to_string(),
        ],
        vec![
            "backup".to_string(),
            "list".to_string(),
            "--runtime".to_string(),
            "--artifact-dir".to_string(),
            artifact_dir.display().to_string(),
            "--json".to_string(),
        ],
        vec![
            "backup".to_string(),
            "verify".to_string(),
            "8206".to_string(),
            "--runtime".to_string(),
            "--artifact-dir".to_string(),
            artifact_dir.display().to_string(),
            "--json".to_string(),
        ],
        vec![
            "backup".to_string(),
            "cancel".to_string(),
            "8206".to_string(),
            "--runtime".to_string(),
            "--artifact-dir".to_string(),
            artifact_dir.display().to_string(),
            "--json".to_string(),
        ],
    ] {
        let output = run_cli_owned(args);
        assert_success(&output);
        assert_contains_all(
            &stdout(&output),
            &["\"durable_backend\":true", "\"backup_id\":8206"],
        );
    }

    let _ = fs::remove_dir_all(artifact_dir);
}

#[test]
fn backup_runtime_requires_artifact_dir_for_state_commands() {
    for args in [
        vec!["backup", "status", "8207", "--runtime", "--json"],
        vec!["backup", "list", "--runtime", "--json"],
        vec!["backup", "verify", "8207", "--runtime", "--json"],
        vec!["backup", "cancel", "8207", "--runtime", "--json"],
    ] {
        let output = run_cli_vec(args);
        assert!(
            !output.status.success(),
            "--runtime without --artifact-dir should fail"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("--artifact-dir"));
    }
}

#[test]
fn backup_verify_rejects_manifest_hash_mismatch() {
    let artifact_dir = create_cli_backup_artifact("backup-verify-manifest-mismatch", 8202);
    let manifest_path = backup_manifest_path(&artifact_dir, 8202);
    let mut manifest = fs::read(&manifest_path).expect("manifest exists");
    let last = manifest.last_mut().expect("manifest has bytes");
    *last ^= 0x01;
    fs::write(&manifest_path, manifest).expect("manifest corruption is written");

    let output = run_cli_owned(vec![
        "backup".to_string(),
        "verify".to_string(),
        "8202".to_string(),
        "--artifact-dir".to_string(),
        artifact_dir.display().to_string(),
        "--json".to_string(),
    ]);
    assert!(
        !output.status.success(),
        "backup verify should reject corrupted manifest"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("manifest payload checksum mismatch"));
    let _ = fs::remove_dir_all(artifact_dir);
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
    let args = vec![
        "restore".to_string(),
        "100".to_string(),
        "--pitr-lsn".to_string(),
        "--json".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(err.message(), "--pitr-lsn requires an LSN value");

    let args = vec![
        "restore".to_string(),
        "verify".to_string(),
        "100".to_string(),
        "--pitr-lsn".to_string(),
        "--json".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(err.message(), "--pitr-lsn requires an LSN value");

    let args = vec![
        "restore".to_string(),
        "100".to_string(),
        "--pitr-policy".to_string(),
        "super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));
}

#[test]
fn restore_dry_run_json_requires_artifact_and_stays_preview_only() {
    let missing_artifact = run_cli([
        "restore",
        "100",
        "--pitr-lsn",
        "1500",
        "--dry-run",
        "--json",
    ]);
    assert!(
        !missing_artifact.status.success(),
        "restore dry-run without --artifact should fail"
    );

    let artifact_dir = create_cli_backup_artifact("restore-dry-run-json", 8104);
    let output = run_cli_owned(vec![
        "restore".to_string(),
        "8104".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-lsn".to_string(),
        "1500".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.restore.start.v1\"",
            "\"contract_preview\":true",
            "\"durable_backend\":true",
            "\"requires_restore_orchestrator\":true",
            "\"dry_run\":true",
            "\"would_restore\":false",
            "\"restore_id\":null",
            "\"backup_id\":8104",
            "\"pitr_target_lsn\":1500",
            "\"preflight_validated\":true",
            "\"replay_segments\":[",
            "no durable restore was orchestrated",
        ],
    );
    let _ = fs::remove_dir_all(artifact_dir);
}

#[test]
fn restore_requires_explicit_pitr_or_policy() {
    let artifact_dir = create_cli_backup_artifact("restore-requires-pitr", 8203);
    let output = run_cli_owned(vec![
        "restore".to_string(),
        "8203".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert!(
        !output.status.success(),
        "restore dry-run must require explicit PITR target or policy"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("explicit --pitr-lsn"));
    let _ = fs::remove_dir_all(artifact_dir);
}

#[test]
fn restore_rejects_lsn_out_of_range() {
    let artifact_dir = create_cli_backup_artifact("restore-lsn-out-of-range", 8204);
    let output = run_cli_owned(vec![
        "restore".to_string(),
        "8204".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-lsn".to_string(),
        "2001".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert!(
        !output.status.success(),
        "restore should reject PITR outside artifact WAL range"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("within backup WAL archive range"));
    let _ = fs::remove_dir_all(artifact_dir);
}

#[test]
fn restore_dry_run_outputs_replay_segments() {
    let artifact_dir = create_cli_backup_artifact("restore-replay-segments", 8205);
    let output = run_cli_owned(vec![
        "restore".to_string(),
        "8205".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-lsn".to_string(),
        "1750".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"pitr_target_lsn\":1750",
            "\"preflight_validated\":true",
            "\"replay_segments\":[",
            "\"sequence_index\":0",
            "\"sequence_index\":1",
            "\"segment_id\":10",
            "\"segment_id\":11",
            "\"contains_pitr_target\":true",
        ],
    );
    let _ = fs::remove_dir_all(artifact_dir);
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

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn run_cli_vec(args: Vec<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn run_cli_owned(args: Vec<String>) -> Output {
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

fn create_cli_backup_artifact(test_name: &str, backup_id: u64) -> PathBuf {
    let artifact_dir = temp_artifact_dir(test_name);
    let output = run_cli_owned(vec![
        "backup".to_string(),
        "start".to_string(),
        "--runtime".to_string(),
        "--artifact-dir".to_string(),
        artifact_dir.display().to_string(),
        "--backup-id".to_string(),
        backup_id.to_string(),
        "--json".to_string(),
    ]);
    assert_success(&output);
    artifact_dir
}

fn temp_artifact_dir(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "andromeda-cli-{test_name}-{}-{nonce}",
        std::process::id()
    ))
}

fn backup_manifest_path(root: &std::path::Path, backup_id: u64) -> PathBuf {
    root.join(format!("backup-{backup_id:016x}"))
        .join("backup.manifest")
}
