use super::execution::{run_restore_command, run_restore_start, run_restore_status};
use crate::diagnostic_json::json_string;

#[test]
fn restore_start_requires_backup_id() {
    let result = run_restore_start(&[]);
    assert!(result.is_err());
}

#[test]
fn restore_start_accepts_valid_backup_id() {
    let result = run_restore_start(&[
        "100".to_string(),
        "--artifact".to_string(),
        "target/backup-100.manifest".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn restore_start_rejects_invalid_backup_id() {
    let result = run_restore_start(&["not_a_number".to_string()]);
    assert!(result.is_err());
}

#[test]
fn restore_start_with_pitr_lsn() {
    let result = run_restore_start(&[
        "100".to_string(),
        "--artifact".to_string(),
        "target/backup-100.manifest".to_string(),
        "--pitr-lsn".to_string(),
        "2097152".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn restore_start_rejects_invalid_pitr_lsn() {
    let result = run_restore_start(&[
        "100".to_string(),
        "--pitr-lsn".to_string(),
        "not_an_lsn".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn restore_start_rejects_unexpected_argument() {
    let result = run_restore_start(&["100".to_string(), "extra".to_string()]);
    assert!(result.is_err());
}

#[test]
fn restore_start_requires_dry_run_without_orchestrator() {
    let result = run_restore_start(&[
        "100".to_string(),
        "--artifact".to_string(),
        "target/backup-100.manifest".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn restore_start_dry_run_requires_artifact() {
    let result = run_restore_start(&["100".to_string(), "--dry-run".to_string()]);
    assert!(result.is_err());
}

#[test]
fn restore_start_rejects_flag_as_artifact() {
    let result = run_restore_start(&[
        "100".to_string(),
        "--artifact".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn restore_status_requires_restore_id() {
    let result = run_restore_status(&[]);
    assert!(result.is_err());
}

#[test]
fn restore_status_accepts_valid_id() {
    let result = run_restore_status(&["200".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn restore_status_rejects_invalid_id() {
    let result = run_restore_status(&["not_a_number".to_string()]);
    assert!(result.is_err());
}

#[test]
fn restore_status_accepts_json_output() {
    let result = run_restore_status(&["200".to_string(), "--json".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn restore_start_accepts_json_output() {
    let result = run_restore_start(&[
        "100".to_string(),
        "--artifact".to_string(),
        "target/backup-100.manifest".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ]);
    assert!(result.is_err());
}

#[test]
fn restore_json_string_escapes_diagnostic_fields() {
    assert_eq!(json_string("restore\rstate"), "\"restore\\rstate\"");
}

#[test]
fn restore_command_help() {
    let result = run_restore_command(&["--help".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn restore_command_no_args_shows_help() {
    let result = run_restore_command(&[]);
    assert!(result.is_ok());
}
