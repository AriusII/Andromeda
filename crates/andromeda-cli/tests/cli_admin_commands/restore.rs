use std::path::Path;

use super::support::{
    assert_contains_all, assert_dispatch_error, assert_dispatch_success,
    assert_dispatch_success_owned, assert_operator_boundary_json, assert_success,
    create_cli_backup_artifact, run_cli, run_cli_owned, stdout,
};

#[test]
fn restore_from_backup_id_executes() {
    let artifact_dir = create_cli_backup_artifact("restore-from-backup-id", 8101);
    assert_dispatch_success_owned(restore_start_args(
        8101,
        artifact_dir.path(),
        "--pitr-policy",
        "latest",
        false,
    ));
}

#[test]
fn restore_with_pitr_lsn_executes() {
    let artifact_dir = create_cli_backup_artifact("restore-with-pitr-lsn", 8102);
    assert_dispatch_success_owned(restore_start_args(
        8102,
        artifact_dir.path(),
        "--pitr-lsn",
        "1500",
        false,
    ));
}

#[test]
fn restore_status_requires_restore_id() {
    assert_dispatch_error(["restore", "status"]);
}

#[test]
fn restore_status_accepts_restore_id() {
    assert_dispatch_success(["restore", "status", "200"]);
}

#[test]
fn restore_status_json_is_operator_diagnostic_surface() {
    let output = run_cli(["restore", "status", "200", "--json"]);
    assert_success(&output);
    let json = stdout(&output);
    assert_operator_boundary_json(&json, "andromeda.cli.restore.status.v1");
    assert_contains_all(
        &json,
        &[
            "\"contract_preview\":true",
            "\"durable_backend\":false",
            "\"runtime_mode\":\"contract_preview/static_restore_orchestrator\"",
            "\"requires_restore_orchestrator\":true",
            "\"restore_id\":200",
        ],
    );
}

#[test]
fn restore_help_command_executes() {
    assert_dispatch_success(["restore", "--help"]);
}

#[test]
fn restore_accepts_json_output() {
    let artifact_dir = create_cli_backup_artifact("restore-json-output", 8103);
    assert_dispatch_success_owned(restore_start_args(
        8103,
        artifact_dir.path(),
        "--pitr-lsn",
        "1500",
        true,
    ));
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
    let output = run_cli_owned(restore_start_args(
        8104,
        artifact_dir.path(),
        "--pitr-lsn",
        "1500",
        true,
    ));
    assert_success(&output);
    let json = stdout(&output);
    assert_operator_boundary_json(&json, "andromeda.cli.restore.start.v1");
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
}

#[test]
fn restore_rejects_lsn_out_of_range() {
    let artifact_dir = create_cli_backup_artifact("restore-lsn-out-of-range", 8204);
    let output = run_cli_owned(restore_start_args(
        8204,
        artifact_dir.path(),
        "--pitr-lsn",
        "2001",
        true,
    ));
    assert!(
        !output.status.success(),
        "restore should reject PITR outside artifact WAL range"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("within backup WAL archive range"));
}

#[test]
fn restore_verify_json_is_operator_control_surface() {
    let artifact_dir = create_cli_backup_artifact("restore-verify-boundary", 8208);
    let output = run_cli_owned(vec![
        "restore".to_string(),
        "verify".to_string(),
        "8208".to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        "--pitr-lsn".to_string(),
        "1750".to_string(),
        "--json".to_string(),
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_operator_boundary_json(&json, "andromeda.cli.restore.verify.v1");
    assert_contains_all(
        &json,
        &[
            "\"durable_backend\":true",
            "\"backup_id\":8208",
            "\"pitr_target_lsn\":1750",
            "\"validation_policy\":\"full\"",
            "\"replay_segments\":[",
            "restore artifact preflight verified; PITR replay plan is bounded",
        ],
    );
}

#[test]
fn restore_dry_run_outputs_replay_segments() {
    let artifact_dir = create_cli_backup_artifact("restore-replay-segments", 8205);
    let output = run_cli_owned(restore_start_args(
        8205,
        artifact_dir.path(),
        "--pitr-lsn",
        "1750",
        true,
    ));
    assert_success(&output);
    let json = stdout(&output);
    assert_operator_boundary_json(&json, "andromeda.cli.restore.start.v1");
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
}

fn restore_start_args(
    backup_id: u64,
    artifact_dir: &Path,
    pitr_flag: &str,
    pitr_value: &str,
    json_output: bool,
) -> Vec<String> {
    let mut args = vec![
        "restore".to_string(),
        backup_id.to_string(),
        "--artifact".to_string(),
        artifact_dir.display().to_string(),
        pitr_flag.to_string(),
        pitr_value.to_string(),
        "--dry-run".to_string(),
    ];
    if json_output {
        args.push("--json".to_string());
    }
    args
}
