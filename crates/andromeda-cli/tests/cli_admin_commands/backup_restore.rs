use andromeda_cli::cmd::dispatch_command;
use std::fs;

use super::support::{
    assert_contains_all, assert_success, backup_manifest_path, create_cli_backup_artifact, run_cli,
    run_cli_owned, run_cli_vec, stdout, temp_artifact_dir,
};

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
