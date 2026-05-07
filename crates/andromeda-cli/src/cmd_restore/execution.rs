use super::output::{
    print_restore_help, print_restore_start, print_restore_status, print_restore_verify,
};
use super::parse::{
    RestorePitrPolicy, parse_restore_start, parse_restore_status, parse_restore_verify,
    restore_pitr_policy_str, restore_validation_policy_str,
};
use super::{
    RestoreReplaySegmentOutput, RestoreStartOutcome, RestoreState, RestoreStatusReport,
    RestoreVerifyOutcome,
};
use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use andromeda_storage::{
    BackupId, FileBackedBackupArtifactStore, Lsn, RestoreValidationPolicy, plan_replay_segments,
    validate_restore_artifact_preflight,
};

#[derive(Debug, Clone)]
struct RestorePreflightOptions {
    backup_id: u64,
    artifact_path: String,
    pitr_target_lsn: Option<u64>,
    pitr_policy: Option<RestorePitrPolicy>,
    validation_policy: RestoreValidationPolicy,
}

#[derive(Debug, Clone)]
struct RestorePreflightReport {
    pitr_target_lsn: u64,
    source_checkpoint_lsn: u64,
    replay_segments: Vec<RestoreReplaySegmentOutput>,
}

/// Parses and executes restore subcommands.
pub(crate) fn run_restore_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            print_restore_help();
            Ok(())
        }
        Some("status") => run_restore_status(&args[1..]),
        Some("verify") => run_restore_verify(&args[1..]),
        Some(_) => run_restore_start(args),
        None => {
            print_restore_help();
            Ok(())
        }
    }
}

/// Restores from backup, optionally to a PITR LSN.
pub(super) fn run_restore_start(args: &[String]) -> AndromedaResult<()> {
    let options = parse_restore_start(args)?;
    let preflight = run_restore_preflight(RestorePreflightOptions {
        backup_id: options.backup_id,
        artifact_path: options.artifact_path.clone(),
        pitr_target_lsn: options.pitr_target_lsn,
        pitr_policy: options.pitr_policy,
        validation_policy: options.validation_policy,
    })?;
    let resolved_pitr_lsn = preflight.pitr_target_lsn;
    let replay_segments = preflight.replay_segments;

    let outcome = RestoreStartOutcome {
        restore_id: None,
        backup_id: options.backup_id,
        artifact_path: options.artifact_path,
        pitr_target_lsn: Some(resolved_pitr_lsn),
        pitr_policy: options
            .pitr_policy
            .map(restore_pitr_policy_str)
            .map(str::to_string),
        validation_policy: restore_validation_policy_str(options.validation_policy).to_string(),
        preflight_validated: true,
        replay_segments,
        contract_preview: true,
        durable_backend: true,
        requires_restore_orchestrator: true,
        dry_run: options.dry_run,
        would_restore: false,
        message: format!(
            "dry-run accepted: restore artifact preflight validated for backup {}{}; no durable restore was orchestrated",
            options.backup_id,
            Some(resolved_pitr_lsn)
                .map(|lsn| format!(" (PITR to LSN {})", lsn))
                .unwrap_or_default()
        ),
    };

    print_restore_start(&outcome, options.json_output);

    Ok(())
}

pub(super) fn run_restore_verify(args: &[String]) -> AndromedaResult<()> {
    let options = parse_restore_verify(args)?;
    let preflight = run_restore_preflight(RestorePreflightOptions {
        backup_id: options.backup_id,
        artifact_path: options.artifact_path.clone(),
        pitr_target_lsn: options.pitr_target_lsn,
        pitr_policy: options.pitr_policy,
        validation_policy: options.validation_policy,
    })?;
    let outcome = RestoreVerifyOutcome {
        backup_id: options.backup_id,
        artifact_path: options.artifact_path,
        pitr_target_lsn: preflight.pitr_target_lsn,
        pitr_policy: options
            .pitr_policy
            .map(restore_pitr_policy_str)
            .map(str::to_string),
        validation_policy: restore_validation_policy_str(options.validation_policy).to_string(),
        source_checkpoint_lsn: preflight.source_checkpoint_lsn,
        replay_segments: preflight.replay_segments,
        message: "restore artifact preflight verified; PITR replay plan is bounded".to_string(),
    };

    print_restore_verify(&outcome, options.json_output);

    Ok(())
}

/// Shows progress of a restore operation.
pub(super) fn run_restore_status(args: &[String]) -> AndromedaResult<()> {
    let options = parse_restore_status(args)?;
    let report = RestoreStatusReport {
        restore_id: options.restore_id,
        state: RestoreState::ContractPreview,
        backup_id: 100,
        pitr_target_lsn: None,
        progress_percent: 0,
        wal_segments_replayed: 0,
        estimated_total_segments: 0,
        start_time: 0,
        elapsed_seconds: 0,
    };

    print_restore_status(&report, options.json_output);

    Ok(())
}

fn run_restore_preflight(
    options: RestorePreflightOptions,
) -> AndromedaResult<RestorePreflightReport> {
    let backup_id = BackupId::new(options.backup_id);
    let pitr_target_lsn = resolve_restore_pitr_target(&options, backup_id)?;
    let preflight = validate_restore_artifact_preflight(
        &options.artifact_path,
        backup_id,
        Lsn::new(pitr_target_lsn),
        options.validation_policy,
    )
    .map_err(|error| {
        cli_error(format!(
            "restore artifact preflight failed for backup {}: {}",
            options.backup_id,
            error.message()
        ))
    })?;

    let store = open_existing_restore_store(&options.artifact_path)?;
    let record = store
        .validate_artifact_directory(backup_id)
        .map_err(|error| {
            cli_error(format!(
                "restore replay planning failed for backup {}: {}",
                options.backup_id,
                error.message()
            ))
        })?;
    let descriptors = record.wal_segment_descriptors().map_err(|error| {
        cli_error(format!(
            "restore replay planning failed for backup {}: {}",
            options.backup_id,
            error.message()
        ))
    })?;
    let replay_segments =
        plan_replay_segments(&record.manifest, Lsn::new(pitr_target_lsn), &descriptors).map_err(
            |error| {
                cli_error(format!(
                    "restore replay planning failed for backup {}: {}",
                    options.backup_id,
                    error.message()
                ))
            },
        )?;

    Ok(RestorePreflightReport {
        pitr_target_lsn,
        source_checkpoint_lsn: preflight.source_checkpoint_lsn.get(),
        replay_segments: replay_segments
            .into_iter()
            .map(|segment| RestoreReplaySegmentOutput {
                sequence_index: segment.sequence_index,
                segment_id: segment.segment_descriptor.segment_id,
                first_lsn: segment.segment_descriptor.first_lsn.get(),
                last_lsn: segment.segment_descriptor.last_lsn.get(),
                contains_pitr_target: segment.contains_pitr_target,
            })
            .collect(),
    })
}

fn resolve_restore_pitr_target(
    options: &RestorePreflightOptions,
    backup_id: BackupId,
) -> AndromedaResult<u64> {
    if let Some(pitr_target_lsn) = options.pitr_target_lsn {
        return Ok(pitr_target_lsn);
    }

    match options.pitr_policy {
        Some(RestorePitrPolicy::Latest) => {
            let store = open_existing_restore_store(&options.artifact_path)?;
            let record = store.load_artifact_manifest(backup_id).map_err(|error| {
                cli_error(format!(
                    "failed to resolve restore --pitr-policy latest for backup {}: {}",
                    options.backup_id,
                    error.message()
                ))
            })?;
            Ok(record.manifest.wal_archive.end_inclusive.get())
        }
        None => Err(cli_error(
            "restore requires an explicit --pitr-lsn <lsn> or --pitr-policy latest",
        )),
    }
}

fn open_existing_restore_store(path: &str) -> AndromedaResult<FileBackedBackupArtifactStore> {
    FileBackedBackupArtifactStore::open_existing(path).map_err(|error| {
        cli_error(format!(
            "failed to open restore artifact directory `{path}`: {}",
            error.message()
        ))
    })
}
