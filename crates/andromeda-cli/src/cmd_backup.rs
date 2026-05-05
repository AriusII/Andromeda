//! Backup administration commands.
//!
//! Provides CLI commands for:
//! - Starting backup jobs (full or incremental)
//! - Monitoring backup progress
//! - Listing recent backups with metadata

use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use std::time::{SystemTime, UNIX_EPOCH};

/// Serializable backup status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum BackupState {
    Pending,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for BackupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupState::Pending => write!(f, "pending"),
            BackupState::Running => write!(f, "running"),
            BackupState::Completed => write!(f, "completed"),
            BackupState::Failed => write!(f, "failed"),
        }
    }
}

impl BackupState {
    const ALL: [Self; 4] = [Self::Pending, Self::Running, Self::Completed, Self::Failed];
}

/// Backup status report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupStatusReport {
    pub backup_id: u64,
    pub state: BackupState,
    pub progress_percent: u32,
    pub bytes_processed: u64,
    pub estimated_total_bytes: u64,
    pub start_time: u64,
    pub elapsed_seconds: u64,
}

/// Backup list entry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupListEntry {
    pub backup_id: u64,
    pub state: BackupState,
    pub size_bytes: u64,
    pub created_timestamp: u64,
    pub base_lsn: u64,
    pub end_lsn: u64,
}

/// Backup start outcome.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupStartOutcome {
    pub backup_id: u64,
    pub backup_type: String,
    pub destination: Option<String>,
    pub message: String,
}

/// Parses and executes backup subcommands.
pub fn run_backup_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("start") => run_backup_start(&args[1..]),
        Some("status") => run_backup_status(&args[1..]),
        Some("list") => run_backup_list(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_backup_help();
            Ok(())
        }
        Some(cmd) => Err(cli_error(format!(
            "unknown backup subcommand `{cmd}`; run `andromeda-cli backup --help`"
        ))),
        None => {
            print_backup_help();
            Ok(())
        }
    }
}

/// Starts a backup job (full or incremental).
fn run_backup_start(args: &[String]) -> AndromedaResult<()> {
    let mut incremental = false;
    let mut destination: Option<String> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--incremental" => incremental = true,
            "--destination" => {
                i += 1;
                if i >= args.len() {
                    return Err(cli_error("--destination requires a path argument"));
                }
                destination = Some(args[i].clone());
            }
            "--json" => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!("unknown backup start option: {}", opt)));
            }
            _ => {}
        }
        i += 1;
    }

    // MOCK: In a real implementation, this would invoke the backup scheduler.
    let backup_type = if incremental {
        "incremental".to_string()
    } else {
        "full".to_string()
    };

    let backup_id = 100u64; // Mock ID
    let outcome = BackupStartOutcome {
        backup_id,
        backup_type,
        destination: destination.clone(),
        message: format!(
            "Backup {} started{}",
            backup_id,
            destination
                .map(|d| format!(" (destination: {})", d))
                .unwrap_or_default()
        ),
    };

    if json_output {
        print_backup_start_json(&outcome);
    } else {
        println!("✓ {}", outcome.message);
        println!("Backup ID: {}", outcome.backup_id);
        println!("Backup Type: {}", outcome.backup_type);
        if let Some(destination) = &outcome.destination {
            println!("Destination: {}", destination);
        }
    }

    Ok(())
}

/// Shows progress of a backup job.
fn run_backup_status(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "backup status requires <backup-id>; usage: `backup status <backup-id>`",
        ));
    }

    let backup_id: u64 = args[0]
        .parse()
        .map_err(|_| cli_error("backup-id must be an unsigned integer"))?;

    let json_output = has_json_option(&args[1..]);

    // MOCK: In a real implementation, this would query the backup scheduler.
    let report = BackupStatusReport {
        backup_id,
        state: BackupState::Running,
        progress_percent: 65,
        bytes_processed: 1_073_741_824,       // 1 GiB
        estimated_total_bytes: 1_610_612_736, // 1.5 GiB
        start_time: unix_timestamp(),
        elapsed_seconds: 120,
    };

    if json_output {
        print_backup_status_json(&report);
    } else {
        print_backup_status_human(&report);
    }

    Ok(())
}

/// Lists recent backups with sizes and timestamps.
fn run_backup_list(args: &[String]) -> AndromedaResult<()> {
    let mut limit = 10usize;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                i += 1;
                if i >= args.len() {
                    return Err(cli_error("--limit requires a numeric argument"));
                }
                limit = args[i]
                    .parse()
                    .map_err(|_| cli_error("--limit expects an unsigned integer"))?;
            }
            "--json" => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!("unknown backup list option: {}", opt)));
            }
            _ => {}
        }
        i += 1;
    }

    // MOCK: In a real implementation, this would query the backup store.
    let backups = vec![
        BackupListEntry {
            backup_id: 102,
            state: BackupState::Completed,
            size_bytes: 1_610_612_736, // 1.5 GiB
            created_timestamp: unix_timestamp() - 3600,
            base_lsn: 1048576,
            end_lsn: 2097152,
        },
        BackupListEntry {
            backup_id: 101,
            state: BackupState::Completed,
            size_bytes: 1_288_490_189, // ~1.2 GiB
            created_timestamp: unix_timestamp() - 7200,
            base_lsn: 524288,
            end_lsn: 1048576,
        },
        BackupListEntry {
            backup_id: 100,
            state: BackupState::Completed,
            size_bytes: 967_367_641, // ~900 MiB
            created_timestamp: unix_timestamp() - 10800,
            base_lsn: 0,
            end_lsn: 524288,
        },
    ];

    let backups_to_show: Vec<_> = backups.iter().take(limit).collect();

    if json_output {
        print_backup_list_json(&backups_to_show);
    } else {
        print_backup_list_human(&backups_to_show);
    }

    Ok(())
}

fn print_backup_help() {
    println!("Andromeda backup administration commands");
    println!();
    println!("USAGE: andromeda-cli backup <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  start               Start a new backup job");
    println!("  status <backup-id>  Show backup progress and state");
    println!("  list                List recent backups with metadata");
    println!();
    println!("OPTIONS:");
    println!("  --incremental       Perform incremental backup (default: full)");
    println!("  --destination <path> Backup destination directory");
    println!("  --limit <n>         Limit backup list to N entries (default: 10)");
    println!("  --json              Emit diagnostic machine-readable JSON output");
    println!("  -h, --help          Show this help message");
    println!();
    println!(
        "STATES: {}",
        BackupState::ALL
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn has_json_option(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

fn print_backup_status_human(report: &BackupStatusReport) {
    println!("Backup Status Report");
    println!("====================");
    println!("Backup ID: {}", report.backup_id);
    println!("State: {}", report.state);
    println!("Progress: {}%", report.progress_percent);
    println!("Start Time: {}", report.start_time);
    println!(
        "Bytes Processed: {} / {} ({:.2} MiB / {:.2} MiB)",
        report.bytes_processed,
        report.estimated_total_bytes,
        report.bytes_processed as f64 / 1_048_576.0,
        report.estimated_total_bytes as f64 / 1_048_576.0
    );
    println!("Elapsed: {}s", report.elapsed_seconds);
}

fn print_backup_list_human(backups: &[&BackupListEntry]) {
    println!("Recent Backups");
    println!("==============");
    println!(
        "{:<10} {:<12} {:<20} {:<15} {:<15} {:<15}",
        "Backup ID", "State", "Size", "Created", "Base LSN", "End LSN"
    );
    println!("{}", "-".repeat(98));
    for backup in backups {
        println!(
            "{:<10} {:<12} {:<20} {:<15} {:<15} {:<15}",
            backup.backup_id,
            backup.state,
            format_bytes(backup.size_bytes),
            backup.created_timestamp,
            backup.base_lsn,
            backup.end_lsn
        );
    }
}

fn print_backup_start_json(outcome: &BackupStartOutcome) {
    println!(
        "{{\"backup_id\":{},\"backup_type\":{},\"destination\":{},\"message\":{}}}",
        outcome.backup_id,
        json_string(&outcome.backup_type),
        json_option_string(outcome.destination.as_deref()),
        json_string(&outcome.message),
    );
}

fn print_backup_status_json(report: &BackupStatusReport) {
    println!(
        "{{\"backup_id\":{},\"state\":{},\"progress_percent\":{},\"bytes_processed\":{},\"estimated_total_bytes\":{},\"start_time\":{},\"elapsed_seconds\":{}}}",
        report.backup_id,
        json_string(&report.state.to_string()),
        report.progress_percent,
        report.bytes_processed,
        report.estimated_total_bytes,
        report.start_time,
        report.elapsed_seconds,
    );
}

fn print_backup_list_json(backups: &[&BackupListEntry]) {
    let entries = backups
        .iter()
        .map(|backup| {
            format!(
                "{{\"backup_id\":{},\"state\":{},\"size_bytes\":{},\"created_timestamp\":{},\"base_lsn\":{},\"end_lsn\":{}}}",
                backup.backup_id,
                json_string(&backup.state.to_string()),
                backup.size_bytes,
                backup.created_timestamp,
                backup.base_lsn,
                backup.end_lsn,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!("[{}]", entries);
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
    let mut size = bytes as f64;
    for unit in UNITS {
        if size < 1024.0 {
            return format!("{:.2} {}", size, unit);
        }
        size /= 1024.0;
    }
    format!("{:.2} TiB", size)
}

fn json_option_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", escape_json_str(value))
}

fn escape_json_str(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_start_returns_ok() {
        let result = run_backup_start(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn backup_start_with_incremental_flag() {
        let result = run_backup_start(&["--incremental".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn backup_start_with_destination() {
        let result = run_backup_start(&["--destination".to_string(), "/backup/dest".to_string()]);
        assert!(result.is_ok());
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
}
