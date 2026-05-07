mod reporting;
mod runtime_plan;
mod validation;

use super::output::print_backup_help;
use super::parse::{
    parse_backup_cancel, parse_backup_list, parse_backup_start, parse_backup_status,
    parse_backup_verify,
};
use super::{
    BackupCancelOutcome, BackupListEntry, BackupStartOutcome, BackupState, BackupStatusReport,
    BackupVerifyOutcome,
};
use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use reporting::{print_cancel, print_list, print_start, print_status, print_verify};
use runtime_plan::{execute_file_backed_backup, generate_backup_id};
use validation::{
    artifact_total_bytes, list_file_backed_backup_entries, validate_existing_backup_artifact,
};

/// Parses and executes backup subcommands.
pub(crate) fn run_backup_command(args: &[String]) -> AndromedaResult<()> {
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
pub(super) fn run_backup_start(args: &[String]) -> AndromedaResult<()> {
    let options = parse_backup_start(args)?;

    if !options.dry_run && options.runtime {
        let artifact_dir = options.artifact_dir.ok_or_else(|| {
            cli_error(
                "backup start --runtime requires --artifact-dir <dir> for file-backed artifacts",
            )
        })?;
        let backup_id = options.backup_id.unwrap_or_else(generate_backup_id);
        let report = execute_file_backed_backup(&artifact_dir, backup_id)?;

        let outcome = BackupStartOutcome {
            backup_id: Some(report.backup_id),
            backup_type: options.backup_type,
            destination: options.destination,
            artifact_dir: Some(report.artifact_root.clone()),
            manifest_path: Some(report.manifest_path),
            snapshot_path: Some(report.snapshot_path),
            wal_segment_paths: report.wal_segment_paths,
            base_lsn: Some(report.base_lsn),
            end_lsn: Some(report.end_lsn),
            contract_preview: false,
            durable_backend: true,
            requires_storage_scheduler: false,
            dry_run: false,
            would_start: true,
            message: format!(
                "file-backed backup execution plan {} persisted to {}",
                report.backup_id, report.artifact_root
            ),
        };

        print_start(&outcome, options.json_output);
        return Ok(());
    }

    if !options.dry_run {
        return Err(cli_error(
            "backup start requires --dry-run for contract preview or --runtime --artifact-dir <dir> for file-backed execution",
        ));
    }

    let outcome = backup_start_contract_preview_outcome(
        options.backup_type,
        options.destination,
        options.artifact_dir,
        options.dry_run,
    );

    print_start(&outcome, options.json_output);
    Ok(())
}

/// Shows progress of a backup job.
pub(super) fn run_backup_status(args: &[String]) -> AndromedaResult<()> {
    let options = parse_backup_status(args)?;

    if let Some(artifact_dir) = options.artifact_dir {
        let artifact = validate_existing_backup_artifact(
            &artifact_dir,
            options.backup_id,
            "failed to read backup artifact status for",
        )?;
        let record = &artifact.record;
        let total_bytes = artifact_total_bytes(record);
        let report = BackupStatusReport {
            backup_id: options.backup_id,
            state: BackupState::Completed,
            progress_percent: 100,
            bytes_processed: total_bytes,
            estimated_total_bytes: total_bytes,
            start_time: record.manifest.created_epoch,
            elapsed_seconds: 0,
            contract_preview: false,
            durable_backend: true,
            requires_storage_scheduler: false,
            artifact_root: Some(artifact.artifact_root(options.backup_id)),
            manifest_path: Some(record.manifest_path.display().to_string()),
            wal_segment_count: record.artifact_set.wal_segments.len(),
            base_lsn: record.manifest.wal_archive.start.get(),
            end_lsn: record.manifest.wal_archive.end_inclusive.get(),
            message: "file-backed backup artifact is complete and checksum-valid".to_string(),
        };

        print_status(&report, options.json_output);
        return Ok(());
    }

    let report = BackupStatusReport {
        backup_id: options.backup_id,
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

    print_status(&report, options.json_output);
    Ok(())
}

/// Lists recent backups with sizes and timestamps.
pub(super) fn run_backup_list(args: &[String]) -> AndromedaResult<()> {
    let options = parse_backup_list(args)?;

    if let Some(artifact_dir) = options.artifact_dir {
        let backups = list_file_backed_backup_entries(&artifact_dir)?;
        let backups_to_show: Vec<_> = backups.iter().take(options.limit).collect();
        print_list(&backups_to_show, false, true, false, options.json_output);
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

    let backups_to_show: Vec<_> = backups.iter().take(options.limit).collect();
    print_list(&backups_to_show, true, false, true, options.json_output);
    Ok(())
}

pub(super) fn run_backup_verify(args: &[String]) -> AndromedaResult<()> {
    let options = parse_backup_verify(args)?;
    let artifact = validate_existing_backup_artifact(
        &options.artifact_dir,
        options.backup_id,
        "backup verify failed for",
    )?;
    let record = &artifact.record;
    let outcome = BackupVerifyOutcome {
        backup_id: options.backup_id,
        artifact_dir: artifact.artifact_root(options.backup_id),
        manifest_path: record.manifest_path.display().to_string(),
        snapshot_path: record.snapshot_path.display().to_string(),
        wal_segment_count: record.artifact_set.wal_segments.len(),
        snapshot_bytes: record.artifact_set.cold_snapshot.artifact.byte_len,
        wal_archive_bytes: record.wal_archive_evidence.total_bytes,
        base_lsn: record.manifest.wal_archive.start.get(),
        end_lsn: record.manifest.wal_archive.end_inclusive.get(),
        message: "backup artifact manifest, snapshot, and WAL checksums verified".to_string(),
    };

    print_verify(&outcome, options.json_output);

    Ok(())
}

pub(super) fn run_backup_cancel(args: &[String]) -> AndromedaResult<()> {
    let options = parse_backup_cancel(args)?;

    let outcome = if let Some(artifact_dir) = options.artifact_dir {
        let artifact = validate_existing_backup_artifact(
            &artifact_dir,
            options.backup_id,
            "backup cancel could not inspect file-backed artifact",
        )?;
        BackupCancelOutcome {
            backup_id: options.backup_id,
            artifact_dir: Some(artifact.artifact_root(options.backup_id)),
            contract_preview: false,
            durable_backend: true,
            dry_run: options.dry_run,
            cancelled: false,
            message: "file-backed backup artifact is already materialized; no scheduler cancellation was applied".to_string(),
        }
    } else {
        if !options.dry_run {
            return Err(cli_error(
                "backup cancel requires --dry-run for contract preview or --runtime --artifact-dir <dir> for file-backed inspection",
            ));
        }
        BackupCancelOutcome {
            backup_id: options.backup_id,
            artifact_dir: None,
            contract_preview: true,
            durable_backend: false,
            dry_run: options.dry_run,
            cancelled: false,
            message: "dry-run accepted: cancel contract validated; no durable backup scheduler was contacted".to_string(),
        }
    };

    print_cancel(&outcome, options.json_output);

    Ok(())
}

fn backup_start_contract_preview_outcome(
    backup_type: String,
    destination: Option<String>,
    artifact_dir: Option<String>,
    dry_run: bool,
) -> BackupStartOutcome {
    let destination_suffix = destination
        .as_deref()
        .map(|destination| format!(" (destination: {destination})"))
        .unwrap_or_default();
    BackupStartOutcome {
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
    }
}
