use super::execution::{run_backup_command, run_backup_list, run_backup_start, run_backup_status};
use super::output::format_bytes;
use crate::diagnostic_json::json_string;

#[test]
fn backup_start_returns_ok() {
    let result = run_backup_start(&["--dry-run".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_start_with_incremental_flag() {
    let result = run_backup_start(&["--incremental".to_string(), "--dry-run".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_start_with_destination() {
    let result = run_backup_start(&[
        "--destination".to_string(),
        "/backup/dest".to_string(),
        "--dry-run".to_string(),
    ]);
    assert!(result.is_ok());
}

#[test]
fn backup_start_requires_dry_run_without_scheduler() {
    let result = run_backup_start(&[]);
    assert!(result.is_err());
}

#[test]
fn backup_start_rejects_flag_as_destination() {
    let result = run_backup_start(&["--destination".to_string(), "--json".to_string()]);
    assert!(result.is_err());
}

#[test]
fn backup_start_rejects_unexpected_argument() {
    let result = run_backup_start(&["extra".to_string()]);
    assert!(result.is_err());
}

#[test]
fn backup_status_requires_backup_id() {
    let result = run_backup_status(&[]);
    assert!(result.is_err());
}

#[test]
fn backup_status_accepts_valid_id() {
    let result = run_backup_status(&["100".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_status_rejects_invalid_id() {
    let result = run_backup_status(&["not_a_number".to_string()]);
    assert!(result.is_err());
}

#[test]
fn backup_list_returns_ok() {
    let result = run_backup_list(&[]);
    assert!(result.is_ok());
}

#[test]
fn backup_list_with_limit() {
    let result = run_backup_list(&["--limit".to_string(), "5".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_list_rejects_zero_limit() {
    let result = run_backup_list(&["--limit".to_string(), "0".to_string()]);
    assert!(result.is_err());
}

#[test]
fn backup_list_rejects_unexpected_argument() {
    let result = run_backup_list(&["extra".to_string()]);
    assert!(result.is_err());
}

#[test]
fn backup_list_accepts_json_output() {
    let result = run_backup_list(&["--json".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_status_accepts_json_output() {
    let result = run_backup_status(&["100".to_string(), "--json".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_json_string_escapes_diagnostic_fields() {
    assert_eq!(
        json_string("path\\with\"quote"),
        "\"path\\\\with\\\"quote\""
    );
}

#[test]
fn backup_command_unknown_subcommand() {
    let result = run_backup_command(&["unknown".to_string()]);
    assert!(result.is_err());
}

#[test]
fn backup_command_help() {
    let result = run_backup_command(&["--help".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn backup_command_no_args_shows_help() {
    let result = run_backup_command(&[]);
    assert!(result.is_ok());
}

#[test]
fn format_bytes_displays_human_readable() {
    assert_eq!(format_bytes(512), "512.00 B");
    assert_eq!(format_bytes(1024), "1.00 KiB");
    assert_eq!(format_bytes(1_048_576), "1.00 MiB");
    assert_eq!(format_bytes(1_073_741_824), "1.00 GiB");
}
