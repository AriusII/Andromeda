use super::{
    BackupCancelOutcome, BackupListEntry, BackupStartOutcome, BackupState, BackupStatusReport,
    BackupVerifyOutcome,
};
use crate::diagnostic_json::{json_option_string, json_option_u64, json_string, json_string_array};

pub(super) fn print_backup_help() {
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

pub(super) fn print_backup_status_human(report: &BackupStatusReport) {
    print_heading("Backup Status Report");
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

pub(super) fn print_backup_list_human(
    backups: &[&BackupListEntry],
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
) {
    if contract_preview {
        print_heading("Backup List Contract Preview");
    } else {
        print_heading("Recent Backups");
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

pub(super) fn print_backup_start_human(outcome: &BackupStartOutcome) {
    if outcome.contract_preview {
        print_heading("Backup Contract Preview");
    } else {
        print_heading("Backup Execution Plan");
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

pub(super) fn print_backup_start_json(outcome: &BackupStartOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.backup.start.v1\",\"contract_preview\":{},\"durable_backend\":{},\"requires_storage_scheduler\":{},\"dry_run\":{},\"would_start\":{},\"backup_id\":{},\"backup_type\":{},\"destination\":{},\"artifact_dir\":{},\"manifest_path\":{},\"snapshot_path\":{},\"wal_segment_paths\":{},\"base_lsn\":{},\"end_lsn\":{},\"message\":{}}}",
        outcome.contract_preview,
        outcome.durable_backend,
        outcome.requires_storage_scheduler,
        outcome.dry_run,
        outcome.would_start,
        json_option_u64(outcome.backup_id),
        json_string(&outcome.backup_type),
        json_option_string(outcome.destination.as_deref()),
        json_option_string(outcome.artifact_dir.as_deref()),
        json_option_string(outcome.manifest_path.as_deref()),
        json_option_string(outcome.snapshot_path.as_deref()),
        json_string_array(&outcome.wal_segment_paths),
        json_option_u64(outcome.base_lsn),
        json_option_u64(outcome.end_lsn),
        json_string(&outcome.message),
    );
}

pub(super) fn print_backup_status_json(report: &BackupStatusReport) {
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

pub(super) fn print_backup_list_json(
    backups: &[&BackupListEntry],
    contract_preview: bool,
    durable_backend: bool,
    requires_storage_scheduler: bool,
) {
    let backups_json = json_array(backups, |backup| {
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
    });
    println!(
        "{{\"schema\":\"andromeda.cli.backup.list.v1\",\"contract_preview\":{},\"durable_backend\":{},\"durable_state_loaded\":{},\"runtime_mode\":{},\"requires_storage_scheduler\":{},\"backups\":{},\"message\":{}}}",
        contract_preview,
        durable_backend,
        durable_backend,
        json_string(output_runtime_mode(contract_preview, durable_backend)),
        requires_storage_scheduler,
        backups_json,
        json_string(if durable_backend {
            "file-backed backup artifact listing completed"
        } else {
            "contract preview: durable backup listing backend is not wired; showing static command contract only"
        })
    );
}

pub(super) fn print_backup_verify_human(outcome: &BackupVerifyOutcome) {
    print_heading("Backup Verify");
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

pub(super) fn print_backup_verify_json(outcome: &BackupVerifyOutcome) {
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

pub(super) fn print_backup_cancel_human(outcome: &BackupCancelOutcome) {
    print_heading("Backup Cancel");
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

pub(super) fn print_backup_cancel_json(outcome: &BackupCancelOutcome) {
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

pub(super) fn format_bytes(bytes: u64) -> String {
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

fn output_runtime_mode(contract_preview: bool, durable_backend: bool) -> &'static str {
    match (contract_preview, durable_backend) {
        (true, false) => "contract_preview/static_scheduler",
        (false, true) => "file_backed_runtime",
        (true, true) => "dry_run_file_backed_preflight",
        _ => "scaffold",
    }
}

fn print_heading(title: &str) {
    println!("{title}");
    println!("{}", "=".repeat(title.len()));
}

fn json_array<T>(items: &[T], render: impl FnMut(&T) -> String) -> String {
    let entries = items.iter().map(render).collect::<Vec<_>>().join(",");
    format!("[{}]", entries)
}
