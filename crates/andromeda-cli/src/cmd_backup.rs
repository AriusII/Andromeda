use crate::diagnostic_json::{JSON_FLAG, json_option_string, json_string};
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64, parse_usize};
use andromeda_core::AndromedaResult;
use andromeda_storage::{
    AllocationId, BackupExecutionPlan, BackupId, BackupManifest, BackupResourceLimits,
    ColdSnapshotBoundary, ExtentCopyTask, ExtentDescriptor, ExtentId, ExtentState,
    FileBackedBackupArtifactStore, Lsn, ObjectId, PageId, PageSize, SegmentId, StorageTier,
    WAL_FORMAT_VERSION, WalArchiveRange, WalSegmentCopyTask, WalSegmentDescriptor,
};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const RUNTIME_SNAPSHOT_BYTES: &[u8] = b"andromeda-cli file-backed backup snapshot fixture v1";
const RUNTIME_WAL_SEGMENT_0_BYTES: &[u8] = b"andromeda-cli file-backed backup wal segment 0 v1";
const RUNTIME_WAL_SEGMENT_1_BYTES: &[u8] = b"andromeda-cli file-backed backup wal segment 1 v1";

/// Backup status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupState {
    ContractPreview,
    Pending,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for BackupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupState::ContractPreview => write!(f, "contract_preview"),
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
#[derive(Debug, Clone)]
pub struct BackupStatusReport {
    pub backup_id: u64,
    pub state: BackupState,
    pub progress_percent: u32,
    pub bytes_processed: u64,
    pub estimated_total_bytes: u64,
    pub start_time: u64,
    pub elapsed_seconds: u64,
    pub contract_preview: bool,
    pub durable_backend: bool,
    pub requires_storage_scheduler: bool,
    pub artifact_root: Option<String>,
    pub manifest_path: Option<String>,
    pub wal_segment_count: usize,
    pub base_lsn: u64,
    pub end_lsn: u64,
    pub message: String,
}

/// Backup list entry.
#[derive(Debug, Clone)]
pub struct BackupListEntry {
    pub backup_id: u64,
    pub state: BackupState,
    pub size_bytes: u64,
    pub created_timestamp: u64,
    pub base_lsn: u64,
    pub end_lsn: u64,
    pub artifact_root: Option<String>,
}

/// Backup start outcome.
#[derive(Debug, Clone)]
pub struct BackupStartOutcome {
    pub backup_id: Option<u64>,
    pub backup_type: String,
    pub destination: Option<String>,
    pub artifact_dir: Option<String>,
    pub manifest_path: Option<String>,
    pub snapshot_path: Option<String>,
    pub wal_segment_paths: Vec<String>,
    pub base_lsn: Option<u64>,
    pub end_lsn: Option<u64>,
    pub contract_preview: bool,
    pub durable_backend: bool,
    pub requires_storage_scheduler: bool,
    pub dry_run: bool,
    pub would_start: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BackupVerifyOutcome {
    pub backup_id: u64,
    pub artifact_dir: String,
    pub manifest_path: String,
    pub snapshot_path: String,
    pub wal_segment_count: usize,
    pub snapshot_bytes: u64,
    pub wal_archive_bytes: u64,
    pub base_lsn: u64,
    pub end_lsn: u64,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BackupCancelOutcome {
    pub backup_id: u64,
    pub artifact_dir: Option<String>,
    pub contract_preview: bool,
    pub durable_backend: bool,
    pub dry_run: bool,
    pub cancelled: bool,
    pub message: String,
}

/// Parses and executes backup subcommands.
pub fn run_backup_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("start") => run_backup_start(&args[1..]),
        Some("status") => run_backup_status(&args[1..]),
        Some("list") => run_backup_list(&args[1..]),
        Some("verify") => run_backup_verify(&args[1..]),
        Some("cancel") => run_backup_cancel(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_backup_help();
            Ok(())
        }
        Some(_) => Err(cli_error(
            "unknown backup subcommand; run `andromeda-cli backup --help`",
        )),
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
    let mut artifact_dir: Option<String> = None;
    let mut backup_id: Option<u64> = None;
    let mut runtime = false;
    let mut json_output = false;
    let mut dry_run = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--incremental" => incremental = true,
            "--runtime" => runtime = true,
            "--backup-id" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--backup-id requires a numeric argument",
                )?;
                let parsed = parse_u64(value, "--backup-id must be an unsigned integer")?;
                if parsed == 0 {
                    return Err(cli_error("--backup-id must be greater than zero"));
                }
                backup_id = Some(parsed);
            }
            "--artifact-dir" => {
                artifact_dir = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact-dir requires a directory path",
                    )?
                    .to_string(),
                );
            }
            "--destination" => {
                destination = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--destination requires a path argument",
                    )?
                    .to_string(),
                );
            }
            "--dry-run" => dry_run = true,
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup start option; supported options are --incremental, --destination, --artifact-dir, --backup-id, --runtime, --dry-run, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected backup start argument; supported options are --incremental, --destination, --artifact-dir, --backup-id, --runtime, --dry-run, and --json",
                ));
            }
        }
        i += 1;
    }

    let backup_type = if incremental {
        "incremental".to_string()
    } else {
        "full".to_string()
    };

    if !dry_run && runtime {
        let artifact_dir = artifact_dir.ok_or_else(|| {
            cli_error(
                "backup start --runtime requires --artifact-dir <dir> for file-backed artifacts",
            )
        })?;
        let backup_id = backup_id.unwrap_or_else(generate_backup_id);
        let plan = build_runtime_backup_execution_plan(backup_id)?;
        let store = FileBackedBackupArtifactStore::open(&artifact_dir).map_err(|error| {
            cli_error(format!(
                "failed to open backup artifact directory `{artifact_dir}`: {}",
                error.message()
            ))
        })?;
        let wal_segments = [RUNTIME_WAL_SEGMENT_0_BYTES, RUNTIME_WAL_SEGMENT_1_BYTES];
        let report = store
            .write_execution_plan_artifact(&plan, RUNTIME_SNAPSHOT_BYTES, &wal_segments)
            .map_err(|error| {
                cli_error(format!(
                    "failed to write backup execution plan artifact: {}",
                    error.message()
                ))
            })?;

        let outcome = BackupStartOutcome {
            backup_id: Some(report.backup_id.get()),
            backup_type,
            destination,
            artifact_dir: Some(report.artifact_root.display().to_string()),
            manifest_path: Some(report.manifest_path.display().to_string()),
            snapshot_path: Some(report.snapshot_path.display().to_string()),
            wal_segment_paths: report
                .wal_segment_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            base_lsn: Some(plan.manifest.wal_archive.start.get()),
            end_lsn: Some(plan.manifest.wal_archive.end_inclusive.get()),
            contract_preview: false,
            durable_backend: true,
            requires_storage_scheduler: false,
            dry_run: false,
            would_start: true,
            message: format!(
                "file-backed backup execution plan {} persisted to {}",
                report.backup_id.get(),
                report.artifact_root.display()
            ),
        };

        if json_output {
            print_backup_start_json(&outcome);
        } else {
            print_backup_start_human(&outcome);
        }

        return Ok(());
    }

    if !dry_run {
        return Err(cli_error(
            "backup start requires --dry-run for contract preview or --runtime --artifact-dir <dir> for file-backed execution",
        ));
    }

    let destination_suffix = destination
        .as_deref()
        .map(|destination| format!(" (destination: {destination})"))
        .unwrap_or_default();
    let outcome = BackupStartOutcome {
        backup_id: None,
        backup_type,
        destination,
        artifact_dir,
        manifest_path: None,
        snapshot_path: None,
        wal_segment_paths: Vec::new(),
        base_lsn: None,
        end_lsn: None,
        contract_preview: true,
        durable_backend: false,
        requires_storage_scheduler: true,
        dry_run,
        would_start: false,
        message: format!(
            "dry-run accepted: backup contract validated{destination_suffix}; no durable backup was scheduled"
        ),
    };

    if json_output {
        print_backup_start_json(&outcome);
    } else {
        print_backup_start_human(&outcome);
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

    let backup_id = parse_u64(&args[0], "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }

    let mut artifact_dir: Option<String> = None;
    let mut runtime = false;
    let mut json_output = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--runtime" => runtime = true,
            "--artifact-dir" => {
                artifact_dir = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact-dir requires a directory path",
                    )?
                    .to_string(),
                );
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup status option; supported options are --runtime, --artifact-dir, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected backup status argument; supported options are --runtime, --artifact-dir, and --json",
                ));
            }
        }
        i += 1;
    }

    if runtime && artifact_dir.is_none() {
        return Err(cli_error(
            "backup status --runtime requires --artifact-dir <dir> for file-backed state",
        ));
    }

    if let Some(artifact_dir) = artifact_dir {
        let store = open_existing_backup_store(&artifact_dir)?;
        let record = store
            .validate_artifact_directory(BackupId::new(backup_id))
            .map_err(|error| {
                cli_error(format!(
                    "failed to read backup artifact status for {backup_id}: {}",
                    error.message()
                ))
            })?;
        let total_bytes = record
            .artifact_set
            .cold_snapshot
            .artifact
            .byte_len
            .saturating_add(record.wal_archive_evidence.total_bytes);
        let report = BackupStatusReport {
            backup_id,
            state: BackupState::Completed,
            progress_percent: 100,
            bytes_processed: total_bytes,
            estimated_total_bytes: total_bytes,
            start_time: record.manifest.created_epoch,
            elapsed_seconds: 0,
            contract_preview: false,
            durable_backend: true,
            requires_storage_scheduler: false,
            artifact_root: Some(
                store
                    .backup_dir(BackupId::new(backup_id))
                    .display()
                    .to_string(),
            ),
            manifest_path: Some(record.manifest_path.display().to_string()),
            wal_segment_count: record.artifact_set.wal_segments.len(),
            base_lsn: record.manifest.wal_archive.start.get(),
            end_lsn: record.manifest.wal_archive.end_inclusive.get(),
            message: "file-backed backup artifact is complete and checksum-valid".to_string(),
        };

        if json_output {
            print_backup_status_json(&report);
        } else {
            print_backup_status_human(&report);
        }

        return Ok(());
    }

    let report = BackupStatusReport {
        backup_id,
        state: BackupState::ContractPreview,
        progress_percent: 0,
        bytes_processed: 0,
        estimated_total_bytes: 0,
        start_time: 0,
        elapsed_seconds: 0,
        contract_preview: true,
        durable_backend: false,
        requires_storage_scheduler: true,
        artifact_root: None,
        manifest_path: None,
        wal_segment_count: 0,
        base_lsn: 0,
        end_lsn: 0,
        message: "contract preview: durable backup status backend is not wired; showing static command contract only".to_string(),
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
    let mut artifact_dir: Option<String> = None;
    let mut runtime = false;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--runtime" => runtime = true,
            "--artifact-dir" => {
                artifact_dir = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact-dir requires a directory path",
                    )?
                    .to_string(),
                );
            }
            "--limit" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--limit requires a numeric argument",
                )?;
                limit = parse_usize(value, "--limit expects an unsigned integer")?;
                if limit == 0 {
                    return Err(cli_error("--limit must be greater than zero"));
                }
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup list option; supported options are --runtime, --artifact-dir, --limit, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected backup list argument; supported options are --runtime, --artifact-dir, --limit, and --json",
                ));
            }
        }
        i += 1;
    }

    if runtime && artifact_dir.is_none() {
        return Err(cli_error(
            "backup list --runtime requires --artifact-dir <dir> for file-backed state",
        ));
    }

    if let Some(artifact_dir) = artifact_dir {
        let backups = list_file_backed_backup_entries(&artifact_dir)?;
        let backups_to_show: Vec<_> = backups.iter().take(limit).collect();
        if json_output {
            print_backup_list_json(&backups_to_show, false, true, false);
        } else {
            print_backup_list_human(&backups_to_show, false, true, false);
        }
        return Ok(());
    }

    let backups = [
        BackupListEntry {
            backup_id: 102,
            state: BackupState::ContractPreview,
            size_bytes: 0,
            created_timestamp: 0,
            base_lsn: 0,
            end_lsn: 0,
            artifact_root: None,
        },
        BackupListEntry {
            backup_id: 101,
            state: BackupState::ContractPreview,
            size_bytes: 0,
            created_timestamp: 0,
            base_lsn: 0,
            end_lsn: 0,
            artifact_root: None,
        },
        BackupListEntry {
            backup_id: 100,
            state: BackupState::ContractPreview,
            size_bytes: 0,
            created_timestamp: 0,
            base_lsn: 0,
            end_lsn: 0,
            artifact_root: None,
        },
    ];

    let backups_to_show: Vec<_> = backups.iter().take(limit).collect();

    if json_output {
        print_backup_list_json(&backups_to_show, true, false, true);
    } else {
        print_backup_list_human(&backups_to_show, true, false, true);
    }

    Ok(())
}

fn run_backup_verify(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "backup verify requires <backup-id>; usage: `backup verify <backup-id> --artifact-dir <dir>`",
        ));
    }

    let backup_id = parse_u64(&args[0], "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }

    let mut artifact_dir: Option<String> = None;
    let mut runtime = false;
    let mut json_output = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--runtime" => runtime = true,
            "--artifact-dir" => {
                artifact_dir = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact-dir requires a directory path",
                    )?
                    .to_string(),
                );
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup verify option; supported options are --runtime, --artifact-dir, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected backup verify argument; supported options are --runtime, --artifact-dir, and --json",
                ));
            }
        }
        i += 1;
    }

    let artifact_dir = artifact_dir.ok_or_else(|| {
        if runtime {
            cli_error("backup verify --runtime requires --artifact-dir <dir>")
        } else {
            cli_error("backup verify requires --artifact-dir <dir>")
        }
    })?;
    let store = open_existing_backup_store(&artifact_dir)?;
    let record = store
        .validate_artifact_directory(BackupId::new(backup_id))
        .map_err(|error| {
            cli_error(format!(
                "backup verify failed for {backup_id}: {}",
                error.message()
            ))
        })?;
    let outcome = BackupVerifyOutcome {
        backup_id,
        artifact_dir: store
            .backup_dir(BackupId::new(backup_id))
            .display()
            .to_string(),
        manifest_path: record.manifest_path.display().to_string(),
        snapshot_path: record.snapshot_path.display().to_string(),
        wal_segment_count: record.artifact_set.wal_segments.len(),
        snapshot_bytes: record.artifact_set.cold_snapshot.artifact.byte_len,
        wal_archive_bytes: record.wal_archive_evidence.total_bytes,
        base_lsn: record.manifest.wal_archive.start.get(),
        end_lsn: record.manifest.wal_archive.end_inclusive.get(),
        message: "backup artifact manifest, snapshot, and WAL checksums verified".to_string(),
    };

    if json_output {
        print_backup_verify_json(&outcome);
    } else {
        print_backup_verify_human(&outcome);
    }

    Ok(())
}

fn run_backup_cancel(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "backup cancel requires <backup-id>; usage: `backup cancel <backup-id> [--artifact-dir <dir>|--dry-run]`",
        ));
    }

    let backup_id = parse_u64(&args[0], "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }

    let mut artifact_dir: Option<String> = None;
    let mut runtime = false;
    let mut json_output = false;
    let mut dry_run = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--runtime" => runtime = true,
            "--artifact-dir" => {
                artifact_dir = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact-dir requires a directory path",
                    )?
                    .to_string(),
                );
            }
            "--dry-run" => dry_run = true,
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup cancel option; supported options are --runtime, --artifact-dir, --dry-run, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected backup cancel argument; supported options are --runtime, --artifact-dir, --dry-run, and --json",
                ));
            }
        }
        i += 1;
    }

    if runtime && artifact_dir.is_none() {
        return Err(cli_error(
            "backup cancel --runtime requires --artifact-dir <dir> for file-backed state",
        ));
    }

    let outcome = if let Some(artifact_dir) = artifact_dir {
        let store = open_existing_backup_store(&artifact_dir)?;
        store
            .validate_artifact_directory(BackupId::new(backup_id))
            .map_err(|error| {
                cli_error(format!(
                    "backup cancel could not inspect file-backed artifact {backup_id}: {}",
                    error.message()
                ))
            })?;
        BackupCancelOutcome {
            backup_id,
            artifact_dir: Some(store.backup_dir(BackupId::new(backup_id)).display().to_string()),
            contract_preview: false,
            durable_backend: true,
            dry_run,
            cancelled: false,
            message: "file-backed backup artifact is already materialized; no scheduler cancellation was applied".to_string(),
        }
    } else {
        if !dry_run {
            return Err(cli_error(
                "backup cancel requires --dry-run for contract preview or --runtime --artifact-dir <dir> for file-backed inspection",
            ));
        }
        BackupCancelOutcome {
            backup_id,
            artifact_dir: None,
            contract_preview: true,
            durable_backend: false,
            dry_run,
            cancelled: false,
            message: "dry-run accepted: cancel contract validated; no durable backup scheduler was contacted".to_string(),
        }
    };

    if json_output {
        print_backup_cancel_json(&outcome);
    } else {
        print_backup_cancel_human(&outcome);
    }

    Ok(())
}

fn print_backup_help() {
    println!("Andromeda backup administration commands");
    println!();
    println!("USAGE: andromeda-cli backup <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  start               Validate or run file-backed backup execution");
    println!("  status <backup-id>  Show backup progress and state");
    println!("  list                List recent backups with metadata");
    println!("  verify <backup-id>  Verify file-backed backup artifact checksums");
    println!("  cancel <backup-id>  Validate cancellation or inspect file-backed backup state");
    println!();
    println!("OPTIONS:");
    println!("  --incremental       Perform incremental backup (default: full)");
    println!("  --destination <path> Backup destination directory");
    println!("  --runtime           Use file-backed runtime state instead of preview");
    println!("  --artifact-dir <dir> File-backed backup artifact root");
    println!("  --backup-id <id>    Stable backup id for file-backed execution");
    println!("  --dry-run           Validate mutating backup contract without scheduling");
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

fn print_backup_status_human(report: &BackupStatusReport) {
    println!("Backup Status Report");
    println!("====================");
    println!("Backup ID: {}", report.backup_id);
    println!(
        "Runtime Mode: {}",
        output_runtime_mode(report.contract_preview, report.durable_backend)
    );
    println!("Durable State Loaded: {}", report.durable_backend);
    println!("State: {}", report.state);
    println!("Progress: {}%", report.progress_percent);
    println!("Start Time: {}", report.start_time);
    println!("Contract Preview: {}", report.contract_preview);
    println!("Durable Backend: {}", report.durable_backend);
    println!(
        "Requires Storage Scheduler: {}",
        report.requires_storage_scheduler
    );
    if let Some(root) = &report.artifact_root {
        println!("Artifact Root: {root}");
    }
    if let Some(path) = &report.manifest_path {
        println!("Manifest: {path}");
    }
    if report.wal_segment_count > 0 {
        println!("WAL Segments: {}", report.wal_segment_count);
        println!("WAL Range: {}..={}", report.base_lsn, report.end_lsn);
    }
    println!(
        "Bytes Processed: {} / {} ({:.2} MiB / {:.2} MiB)",
        report.bytes_processed,
        report.estimated_total_bytes,
        report.bytes_processed as f64 / 1_048_576.0,
        report.estimated_total_bytes as f64 / 1_048_576.0
    );
    println!("Elapsed: {}s", report.elapsed_seconds);
    println!("{}", report.message);
}

fn print_backup_list_human(
    backups: &[&BackupListEntry],
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
) {
    if contract_preview {
        println!("Backup List Contract Preview");
        println!("============================");
    } else {
        println!("Recent Backups");
        println!("==============");
    }
    println!(
        "Runtime Mode: {}",
        output_runtime_mode(contract_preview, durable_backend)
    );
    println!("Contract Preview: {}", contract_preview);
    println!("Durable Backend: {}", durable_backend);
    println!("Durable State Loaded: {}", durable_backend);
    println!("Requires Storage Scheduler: {}", requires_storage_scheduler);
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

fn print_backup_start_human(outcome: &BackupStartOutcome) {
    if outcome.contract_preview {
        println!("Backup Contract Preview");
        println!("=======================");
    } else {
        println!("Backup Execution Plan");
        println!("=====================");
    }
    println!("{}", outcome.message);
    println!("Contract Preview: {}", outcome.contract_preview);
    println!("Durable Backend: {}", outcome.durable_backend);
    println!(
        "Requires Storage Scheduler: {}",
        outcome.requires_storage_scheduler
    );
    println!("Dry Run: {}", outcome.dry_run);
    println!("Would Start Durable Backup: {}", outcome.would_start);
    println!("Backup Type: {}", outcome.backup_type);
    if let Some(backup_id) = outcome.backup_id {
        println!("Backup ID: {backup_id}");
    }
    if let Some(destination) = &outcome.destination {
        println!("Destination: {destination}");
    }
    if let Some(artifact_dir) = &outcome.artifact_dir {
        println!("Artifact Dir: {artifact_dir}");
    }
    if let Some(path) = &outcome.manifest_path {
        println!("Manifest: {path}");
    }
    if let Some(path) = &outcome.snapshot_path {
        println!("Snapshot: {path}");
    }
    if !outcome.wal_segment_paths.is_empty() {
        println!("WAL Segments: {}", outcome.wal_segment_paths.len());
    }
    if let (Some(start), Some(end)) = (outcome.base_lsn, outcome.end_lsn) {
        println!("WAL Range: {start}..={end}");
    }
}

fn print_backup_start_json(outcome: &BackupStartOutcome) {
    let wal_paths = outcome
        .wal_segment_paths
        .iter()
        .map(|path| json_string(path))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"andromeda.cli.backup.start.v1\",\"contract_preview\":{},\"durable_backend\":{},\"requires_storage_scheduler\":{},\"dry_run\":{},\"would_start\":{},\"backup_id\":{},\"backup_type\":{},\"destination\":{},\"artifact_dir\":{},\"manifest_path\":{},\"snapshot_path\":{},\"wal_segment_paths\":[{}],\"base_lsn\":{},\"end_lsn\":{},\"message\":{}}}",
        outcome.contract_preview,
        outcome.durable_backend,
        outcome.requires_storage_scheduler,
        outcome.dry_run,
        outcome.would_start,
        outcome
            .backup_id
            .map(|backup_id| backup_id.to_string())
            .unwrap_or_else(|| "null".to_string()),
        json_string(&outcome.backup_type),
        json_option_string(outcome.destination.as_deref()),
        json_option_string(outcome.artifact_dir.as_deref()),
        json_option_string(outcome.manifest_path.as_deref()),
        json_option_string(outcome.snapshot_path.as_deref()),
        wal_paths,
        outcome
            .base_lsn
            .map(|lsn| lsn.to_string())
            .unwrap_or_else(|| "null".to_string()),
        outcome
            .end_lsn
            .map(|lsn| lsn.to_string())
            .unwrap_or_else(|| "null".to_string()),
        json_string(&outcome.message),
    );
}

fn print_backup_status_json(report: &BackupStatusReport) {
    println!(
        "{{\"schema\":\"andromeda.cli.backup.status.v1\",\"contract_preview\":{},\"durable_backend\":{},\"durable_state_loaded\":{},\"runtime_mode\":{},\"requires_storage_scheduler\":{},\"backup_id\":{},\"state\":{},\"progress_percent\":{},\"bytes_processed\":{},\"estimated_total_bytes\":{},\"start_time\":{},\"elapsed_seconds\":{},\"artifact_root\":{},\"manifest_path\":{},\"wal_segment_count\":{},\"base_lsn\":{},\"end_lsn\":{},\"message\":{}}}",
        report.contract_preview,
        report.durable_backend,
        report.durable_backend,
        json_string(output_runtime_mode(
            report.contract_preview,
            report.durable_backend
        )),
        report.requires_storage_scheduler,
        report.backup_id,
        json_string(&report.state.to_string()),
        report.progress_percent,
        report.bytes_processed,
        report.estimated_total_bytes,
        report.start_time,
        report.elapsed_seconds,
        json_option_string(report.artifact_root.as_deref()),
        json_option_string(report.manifest_path.as_deref()),
        report.wal_segment_count,
        report.base_lsn,
        report.end_lsn,
        json_string(&report.message),
    );
}

fn print_backup_list_json(
    backups: &[&BackupListEntry],
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
) {
    let entries = backups
        .iter()
        .map(|backup| {
            format!(
                "{{\"backup_id\":{},\"state\":{},\"size_bytes\":{},\"created_timestamp\":{},\"base_lsn\":{},\"end_lsn\":{},\"artifact_root\":{}}}",
                backup.backup_id,
                json_string(&backup.state.to_string()),
                backup.size_bytes,
                backup.created_timestamp,
                backup.base_lsn,
                backup.end_lsn,
                json_option_string(backup.artifact_root.as_deref()),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"andromeda.cli.backup.list.v1\",\"contract_preview\":{},\"durable_backend\":{},\"durable_state_loaded\":{},\"runtime_mode\":{},\"requires_storage_scheduler\":{},\"backups\":[{}],\"message\":{}}}",
        contract_preview,
        durable_backend,
        durable_backend,
        json_string(output_runtime_mode(contract_preview, durable_backend)),
        requires_storage_scheduler,
        entries,
        json_string(if durable_backend {
            "file-backed backup artifact listing completed"
        } else {
            "contract preview: durable backup listing backend is not wired; showing static command contract only"
        })
    );
}

fn output_runtime_mode(contract_preview: bool, durable_backend: bool) -> &'static str {
    match (contract_preview, durable_backend) {
        (true, false) => "contract_preview/static_scheduler",
        (false, true) => "file_backed_runtime",
        (true, true) => "dry_run_file_backed_preflight",
        _ => "scaffold",
    }
}

fn print_backup_verify_human(outcome: &BackupVerifyOutcome) {
    println!("Backup Verify");
    println!("=============");
    println!("Backup ID: {}", outcome.backup_id);
    println!("Artifact Dir: {}", outcome.artifact_dir);
    println!("Manifest: {}", outcome.manifest_path);
    println!("Snapshot: {}", outcome.snapshot_path);
    println!("Snapshot Bytes: {}", outcome.snapshot_bytes);
    println!("WAL Segments: {}", outcome.wal_segment_count);
    println!("WAL Bytes: {}", outcome.wal_archive_bytes);
    println!("WAL Range: {}..={}", outcome.base_lsn, outcome.end_lsn);
    println!("{}", outcome.message);
}

fn print_backup_verify_json(outcome: &BackupVerifyOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.backup.verify.v1\",\"durable_backend\":true,\"backup_id\":{},\"artifact_dir\":{},\"manifest_path\":{},\"snapshot_path\":{},\"wal_segment_count\":{},\"snapshot_bytes\":{},\"wal_archive_bytes\":{},\"base_lsn\":{},\"end_lsn\":{},\"message\":{}}}",
        outcome.backup_id,
        json_string(&outcome.artifact_dir),
        json_string(&outcome.manifest_path),
        json_string(&outcome.snapshot_path),
        outcome.wal_segment_count,
        outcome.snapshot_bytes,
        outcome.wal_archive_bytes,
        outcome.base_lsn,
        outcome.end_lsn,
        json_string(&outcome.message),
    );
}

fn print_backup_cancel_human(outcome: &BackupCancelOutcome) {
    println!("Backup Cancel");
    println!("=============");
    println!("Backup ID: {}", outcome.backup_id);
    println!("Contract Preview: {}", outcome.contract_preview);
    println!("Durable Backend: {}", outcome.durable_backend);
    println!("Dry Run: {}", outcome.dry_run);
    println!("Cancelled: {}", outcome.cancelled);
    if let Some(path) = &outcome.artifact_dir {
        println!("Artifact Dir: {path}");
    }
    println!("{}", outcome.message);
}

fn print_backup_cancel_json(outcome: &BackupCancelOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.backup.cancel.v1\",\"contract_preview\":{},\"durable_backend\":{},\"dry_run\":{},\"cancelled\":{},\"backup_id\":{},\"artifact_dir\":{},\"message\":{}}}",
        outcome.contract_preview,
        outcome.durable_backend,
        outcome.dry_run,
        outcome.cancelled,
        outcome.backup_id,
        json_option_string(outcome.artifact_dir.as_deref()),
        json_string(&outcome.message),
    );
}

fn open_existing_backup_store(path: &str) -> AndromedaResult<FileBackedBackupArtifactStore> {
    FileBackedBackupArtifactStore::open_existing(path).map_err(|error| {
        cli_error(format!(
            "failed to open backup artifact directory `{path}`: {}",
            error.message()
        ))
    })
}

fn list_file_backed_backup_entries(artifact_dir: &str) -> AndromedaResult<Vec<BackupListEntry>> {
    let store = open_existing_backup_store(artifact_dir)?;
    let mut entries = Vec::new();
    let read_dir = fs::read_dir(store.root()).map_err(|error| {
        cli_error(format!(
            "failed to list backup artifact directory `{}`: {error}",
            store.root().display()
        ))
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|error| {
            cli_error(format!(
                "failed to read backup artifact directory entry `{}`: {error}",
                store.root().display()
            ))
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(backup_id) = backup_id_from_dir_name(&path) else {
            continue;
        };
        let record = store
            .validate_artifact_directory(BackupId::new(backup_id))
            .map_err(|error| {
                cli_error(format!(
                    "failed to validate listed backup artifact {backup_id}: {}",
                    error.message()
                ))
            })?;
        let size_bytes = record
            .artifact_set
            .cold_snapshot
            .artifact
            .byte_len
            .saturating_add(record.wal_archive_evidence.total_bytes);
        entries.push(BackupListEntry {
            backup_id,
            state: BackupState::Completed,
            size_bytes,
            created_timestamp: record.manifest.created_epoch,
            base_lsn: record.manifest.wal_archive.start.get(),
            end_lsn: record.manifest.wal_archive.end_inclusive.get(),
            artifact_root: Some(path.display().to_string()),
        });
    }

    entries.sort_by(|left, right| {
        right
            .created_timestamp
            .cmp(&left.created_timestamp)
            .then_with(|| right.backup_id.cmp(&left.backup_id))
    });
    Ok(entries)
}

fn backup_id_from_dir_name(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let hex = name.strip_prefix("backup-")?;
    u64::from_str_radix(hex, 16).ok().filter(|id| *id != 0)
}

fn build_runtime_backup_execution_plan(backup_id: u64) -> AndromedaResult<BackupExecutionPlan> {
    let manifest = BackupManifest {
        backup_id: BackupId::new(backup_id),
        database_id: 42,
        created_epoch: unix_timestamp().max(1),
        snapshot: ColdSnapshotBoundary {
            snapshot_id: 99,
            snapshot_descriptor_hash: [0x5A; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1001),
        },
        wal_archive: WalArchiveRange::new(Lsn::new(1001), Lsn::new(2000)),
        manifest_crc: 777,
    };

    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(10),
        allocation_id: AllocationId::new(20),
        first_page_id: PageId::new(100),
        page_count: 1,
        page_size: PageSize::KiB16,
        state: ExtentState::PublishedCold,
        segment_id: Some(SegmentId::new(1)),
        file_offset: 0,
        allocated_on_disk: true,
    };
    let wal_segment_0 = WalSegmentDescriptor {
        format_version: WAL_FORMAT_VERSION,
        segment_id: 10,
        first_lsn: Lsn::new(1001),
        last_lsn: Lsn::new(1500),
        base_previous_lsn: None,
        record_count: 500,
    };
    let wal_segment_1 = WalSegmentDescriptor {
        format_version: WAL_FORMAT_VERSION,
        segment_id: 11,
        first_lsn: Lsn::new(1501),
        last_lsn: Lsn::new(2000),
        base_previous_lsn: Some(Lsn::new(1500)),
        record_count: 500,
    };

    let total_wal_bytes =
        (RUNTIME_WAL_SEGMENT_0_BYTES.len() + RUNTIME_WAL_SEGMENT_1_BYTES.len()) as u64;
    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: RUNTIME_SNAPSHOT_BYTES.len() as u64,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_segment_0,
                byte_count: RUNTIME_WAL_SEGMENT_0_BYTES.len() as u64,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_segment_1,
                byte_count: RUNTIME_WAL_SEGMENT_1_BYTES.len() as u64,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: true,
        resource_limits: BackupResourceLimits {
            max_total_extent_bytes: 1_000_000,
            max_total_wal_bytes: 1_000_000,
            max_parallel_extent_tasks: 8,
            max_wal_segment_count: 16,
        },
        total_extent_bytes: RUNTIME_SNAPSHOT_BYTES.len() as u64,
        total_wal_bytes,
    };
    plan.validate()?;
    Ok(plan)
}

fn generate_backup_id() -> u64 {
    unix_timestamp().max(1)
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
}
