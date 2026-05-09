use std::{fs, path::Path};

use super::support::{
    assert_contains_all, assert_dispatch_error, assert_dispatch_success,
    assert_operator_boundary_json, assert_success, backup_manifest_path,
    create_cli_backup_artifact, run_cli, run_cli_owned, run_cli_vec, stdout, temp_artifact_dir,
};

#[test]
fn backup_start_command_parses_and_executes() {
    assert_dispatch_success(["backup", "start", "--dry-run"]);
}

#[test]
fn backup_start_with_incremental_flag() {
    assert_dispatch_success(["backup", "start", "--incremental", "--dry-run"]);
}

#[test]
fn backup_start_with_destination_path() {
    assert_dispatch_success([
        "backup",
        "start",
        "--destination",
        "/backup/dest",
        "--dry-run",
    ]);
}

#[test]
fn backup_status_requires_backup_id() {
    assert_dispatch_error(["backup", "status"]);
}

#[test]
fn backup_status_accepts_backup_id() {
    assert_dispatch_success(["backup", "status", "100"]);
}

#[test]
fn backup_list_command_executes() {
    assert_dispatch_success(["backup", "list"]);
}

#[test]
fn backup_list_with_limit_flag() {
    assert_dispatch_success(["backup", "list", "--limit", "5"]);
}

#[test]
fn backup_help_command_executes() {
    assert_dispatch_success(["backup", "--help"]);
}

#[test]
fn backup_status_accepts_json_output() {
    assert_dispatch_success(["backup", "status", "100", "--json"]);
}

#[test]
fn backup_list_accepts_json_output() {
    assert_dispatch_success(["backup", "list", "--json"]);
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
    assert_operator_boundary_json(&json, "andromeda.cli.backup.start.v1");
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
    assert_operator_boundary_json(&json, "andromeda.cli.backup.start.v1");
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
    assert!(backup_manifest_path(artifact_dir.path(), 8201).is_file());
}

#[test]
fn backup_runtime_commands_use_file_backed_artifacts() {
    let artifact_dir = create_cli_backup_artifact("backup-runtime-commands", 8206);

    for (args, expected_schema) in [
        (
            backup_runtime_command("status", Some(8206), artifact_dir.path()),
            "andromeda.cli.backup.status.v1",
        ),
        (
            backup_runtime_command("list", None, artifact_dir.path()),
            "andromeda.cli.backup.list.v1",
        ),
        (
            backup_runtime_command("verify", Some(8206), artifact_dir.path()),
            "andromeda.cli.backup.verify.v1",
        ),
        (
            backup_runtime_command("cancel", Some(8206), artifact_dir.path()),
            "andromeda.cli.backup.cancel.v1",
        ),
    ] {
        let output = run_cli_owned(args);
        assert_success(&output);
        let json = stdout(&output);
        assert_operator_boundary_json(&json, expected_schema);
        assert_contains_all(&json, &["\"durable_backend\":true", "\"backup_id\":8206"]);
    }
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
    let manifest_path = backup_manifest_path(artifact_dir.path(), 8202);
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
}

fn backup_runtime_command(
    subcommand: &str,
    backup_id: Option<u64>,
    artifact_dir: &Path,
) -> Vec<String> {
    let mut args = vec!["backup".to_string(), subcommand.to_string()];
    if let Some(backup_id) = backup_id {
        args.push(backup_id.to_string());
    }
    args.extend([
        "--runtime".to_string(),
        "--artifact-dir".to_string(),
        artifact_dir.display().to_string(),
        "--json".to_string(),
    ]);
    args
}
