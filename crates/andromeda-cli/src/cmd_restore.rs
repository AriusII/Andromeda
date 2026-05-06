use crate::diagnostic_json::{JSON_FLAG, json_option_string, json_option_u64, json_string};
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_core::AndromedaResult;
use andromeda_storage::{
    BackupId, FileBackedBackupArtifactStore, Lsn, RestoreValidationPolicy, plan_replay_segments,
    validate_restore_artifact_preflight,
};

/// Restore state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreState {
    ContractPreview,
    Pending,
    ValidatingManifest,
    ReplayingWal,
    Completed,
    Failed,
}

impl std::fmt::Display for RestoreState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreState::ContractPreview => write!(f, "contract_preview"),
            RestoreState::Pending => write!(f, "pending"),
            RestoreState::ValidatingManifest => write!(f, "validating_manifest"),
            RestoreState::ReplayingWal => write!(f, "replaying_wal"),
            RestoreState::Completed => write!(f, "completed"),
            RestoreState::Failed => write!(f, "failed"),
        }
    }
}

impl RestoreState {
    const ALL: [Self; 5] = [
        Self::Pending,
        Self::ValidatingManifest,
        Self::ReplayingWal,
        Self::Completed,
        Self::Failed,
    ];
}

/// Restore status report.
#[derive(Debug, Clone)]
pub struct RestoreStatusReport {
    pub restore_id: u64,
    pub state: RestoreState,
    pub backup_id: u64,
    pub pitr_target_lsn: Option<u64>,
    pub progress_percent: u32,
    pub wal_segments_replayed: u64,
    pub estimated_total_segments: u64,
    pub start_time: u64,
    pub elapsed_seconds: u64,
}

/// Restore start outcome.
#[derive(Debug, Clone)]
pub struct RestoreStartOutcome {
    pub restore_id: Option<u64>,
    pub backup_id: u64,
    pub artifact_path: String,
    pub pitr_target_lsn: Option<u64>,
    pub pitr_policy: Option<String>,
    pub validation_policy: String,
    pub preflight_validated: bool,
    pub replay_segments: Vec<RestoreReplaySegmentOutput>,
    pub contract_preview: bool,
    pub durable_backend: bool,
    pub requires_restore_orchestrator: bool,
    pub dry_run: bool,
    pub would_restore: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RestoreVerifyOutcome {
    pub backup_id: u64,
    pub artifact_path: String,
    pub pitr_target_lsn: u64,
    pub pitr_policy: Option<String>,
    pub validation_policy: String,
    pub source_checkpoint_lsn: u64,
    pub replay_segments: Vec<RestoreReplaySegmentOutput>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RestoreReplaySegmentOutput {
    pub sequence_index: usize,
    pub segment_id: u64,
    pub first_lsn: u64,
    pub last_lsn: u64,
    pub contains_pitr_target: bool,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestorePitrPolicy {
    Latest,
}

/// Parses and executes restore subcommands.
pub fn run_restore_command(args: &[String]) -> AndromedaResult<()> {
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
fn run_restore_start(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "restore requires <backup-id>; usage: `restore <backup-id> [--pitr-lsn <lsn>]`",
        ));
    }

    let backup_id = parse_u64(&args[0], "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }

    let mut pitr_target_lsn: Option<u64> = None;
    let mut pitr_policy: Option<RestorePitrPolicy> = None;
    let mut validation_policy = RestoreValidationPolicy::Full;
    let mut artifact_path: Option<String> = None;
    let mut json_output = false;
    let mut dry_run = false;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--pitr-lsn" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--pitr-lsn requires an LSN value",
                )?;
                pitr_target_lsn = Some(parse_u64(
                    value,
                    "--pitr-lsn expects an unsigned integer (LSN)",
                )?);
            }
            "--pitr-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--pitr-policy requires a policy value",
                )?;
                pitr_policy = Some(parse_restore_pitr_policy(value)?);
            }
            "--validation-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--validation-policy requires full or minimal",
                )?;
                validation_policy = parse_restore_validation_policy(value)?;
            }
            "--artifact" | "--artifact-path" => {
                artifact_path = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact requires a backup artifact path",
                    )?
                    .to_string(),
                );
            }
            "--dry-run" => dry_run = true,
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown restore option; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, --dry-run, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected restore argument; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, --dry-run, and --json",
                ));
            }
        }
        i += 1;
    }

    if pitr_target_lsn.is_some() && pitr_policy.is_some() {
        return Err(cli_error(
            "restore requires either --pitr-lsn or --pitr-policy, not both",
        ));
    }
    if pitr_target_lsn.is_none() && pitr_policy.is_none() {
        return Err(cli_error(
            "restore requires an explicit --pitr-lsn <lsn> or --pitr-policy latest",
        ));
    }
    if matches!(pitr_target_lsn, Some(0)) {
        return Err(cli_error("--pitr-lsn must be greater than zero"));
    }

    if !dry_run {
        return Err(cli_error(
            "restore is contract preview only until a durable restore orchestrator is wired; rerun with --dry-run",
        ));
    }

    let artifact_path = artifact_path.ok_or_else(|| {
        cli_error(
            "restore dry-run requires --artifact <path> so manifest/artifact validation is explicit",
        )
    })?;

    let preflight = run_restore_preflight(RestorePreflightOptions {
        backup_id,
        artifact_path: artifact_path.clone(),
        pitr_target_lsn,
        pitr_policy,
        validation_policy,
    })?;
    let resolved_pitr_lsn = preflight.pitr_target_lsn;
    let replay_segments = preflight.replay_segments;

    let outcome = RestoreStartOutcome {
        restore_id: None,
        backup_id,
        artifact_path,
        pitr_target_lsn: Some(resolved_pitr_lsn),
        pitr_policy: pitr_policy.map(restore_pitr_policy_str).map(str::to_string),
        validation_policy: restore_validation_policy_str(validation_policy).to_string(),
        preflight_validated: true,
        replay_segments,
        contract_preview: true,
        durable_backend: true,
        requires_restore_orchestrator: true,
        dry_run,
        would_restore: false,
        message: format!(
            "dry-run accepted: restore artifact preflight validated for backup {}{}; no durable restore was orchestrated",
            backup_id,
            Some(resolved_pitr_lsn)
                .map(|lsn| format!(" (PITR to LSN {})", lsn))
                .unwrap_or_default()
        ),
    };

    if json_output {
        print_restore_start_json(&outcome);
    } else {
        println!("Restore Contract Preview");
        println!("========================");
        println!("{}", outcome.message);
        println!("Contract Preview: {}", outcome.contract_preview);
        println!("Durable Backend: {}", outcome.durable_backend);
        println!(
            "Requires Restore Orchestrator: {}",
            outcome.requires_restore_orchestrator
        );
        println!("Dry Run: {}", outcome.dry_run);
        println!("Would Restore: {}", outcome.would_restore);
        println!("Backup ID: {}", outcome.backup_id);
        println!("Artifact Path: {}", outcome.artifact_path);
        if let Some(lsn) = outcome.pitr_target_lsn {
            println!("PITR Target LSN: {}", lsn);
        }
        if let Some(policy) = &outcome.pitr_policy {
            println!("PITR Policy: {policy}");
        }
        println!("Validation Policy: {}", outcome.validation_policy);
        println!("Preflight Validated: {}", outcome.preflight_validated);
        if !outcome.replay_segments.is_empty() {
            println!("Replay Segments:");
            for segment in &outcome.replay_segments {
                println!(
                    "  - #{} segment_id={} lsn={}..={} contains_pitr={}",
                    segment.sequence_index,
                    segment.segment_id,
                    segment.first_lsn,
                    segment.last_lsn,
                    segment.contains_pitr_target
                );
            }
        }
    }

    Ok(())
}

fn run_restore_verify(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "restore verify requires <backup-id>; usage: `restore verify <backup-id> --artifact <dir> (--pitr-lsn <lsn>|--pitr-policy latest)`",
        ));
    }

    let backup_id = parse_u64(&args[0], "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }

    let mut artifact_path: Option<String> = None;
    let mut pitr_target_lsn: Option<u64> = None;
    let mut pitr_policy: Option<RestorePitrPolicy> = None;
    let mut validation_policy = RestoreValidationPolicy::Full;
    let mut json_output = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--artifact" | "--artifact-path" => {
                artifact_path = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact requires a backup artifact directory",
                    )?
                    .to_string(),
                );
            }
            "--pitr-lsn" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--pitr-lsn requires an LSN value",
                )?;
                pitr_target_lsn = Some(parse_u64(
                    value,
                    "--pitr-lsn expects an unsigned integer (LSN)",
                )?);
            }
            "--pitr-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--pitr-policy requires a policy value",
                )?;
                pitr_policy = Some(parse_restore_pitr_policy(value)?);
            }
            "--validation-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--validation-policy requires full or minimal",
                )?;
                validation_policy = parse_restore_validation_policy(value)?;
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown restore verify option; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected restore verify argument; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, and --json",
                ));
            }
        }
        i += 1;
    }

    if pitr_target_lsn.is_some() && pitr_policy.is_some() {
        return Err(cli_error(
            "restore verify requires either --pitr-lsn or --pitr-policy, not both",
        ));
    }
    if pitr_target_lsn.is_none() && pitr_policy.is_none() {
        return Err(cli_error(
            "restore verify requires an explicit --pitr-lsn <lsn> or --pitr-policy latest",
        ));
    }
    if matches!(pitr_target_lsn, Some(0)) {
        return Err(cli_error("--pitr-lsn must be greater than zero"));
    }
    let artifact_path =
        artifact_path.ok_or_else(|| cli_error("restore verify requires --artifact <dir>"))?;

    let preflight = run_restore_preflight(RestorePreflightOptions {
        backup_id,
        artifact_path: artifact_path.clone(),
        pitr_target_lsn,
        pitr_policy,
        validation_policy,
    })?;
    let outcome = RestoreVerifyOutcome {
        backup_id,
        artifact_path,
        pitr_target_lsn: preflight.pitr_target_lsn,
        pitr_policy: pitr_policy.map(restore_pitr_policy_str).map(str::to_string),
        validation_policy: restore_validation_policy_str(validation_policy).to_string(),
        source_checkpoint_lsn: preflight.source_checkpoint_lsn,
        replay_segments: preflight.replay_segments,
        message: "restore artifact preflight verified; PITR replay plan is bounded".to_string(),
    };

    if json_output {
        print_restore_verify_json(&outcome);
    } else {
        print_restore_verify_human(&outcome);
    }

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

    let store =
        FileBackedBackupArtifactStore::open_existing(&options.artifact_path).map_err(|error| {
            cli_error(format!(
                "failed to open restore artifact directory `{}`: {}",
                options.artifact_path,
                error.message()
            ))
        })?;
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
            let store = FileBackedBackupArtifactStore::open_existing(&options.artifact_path)
                .map_err(|error| {
                    cli_error(format!(
                        "failed to open restore artifact directory `{}`: {}",
                        options.artifact_path,
                        error.message()
                    ))
                })?;
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

fn parse_restore_pitr_policy(value: &str) -> AndromedaResult<RestorePitrPolicy> {
    match value {
        "latest" => Ok(RestorePitrPolicy::Latest),
        _ => Err(cli_error("unknown restore PITR policy; expected latest")),
    }
}

fn restore_pitr_policy_str(policy: RestorePitrPolicy) -> &'static str {
    match policy {
        RestorePitrPolicy::Latest => "latest",
    }
}

fn parse_restore_validation_policy(value: &str) -> AndromedaResult<RestoreValidationPolicy> {
    match value {
        "full" => Ok(RestoreValidationPolicy::Full),
        "minimal" => Ok(RestoreValidationPolicy::Minimal),
        _ => Err(cli_error(
            "unknown restore validation policy; expected full or minimal",
        )),
    }
}

fn restore_validation_policy_str(policy: RestoreValidationPolicy) -> &'static str {
    match policy {
        RestoreValidationPolicy::Full => "full",
        RestoreValidationPolicy::Minimal => "minimal",
    }
}

/// Shows progress of a restore operation.
fn run_restore_status(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "restore status requires <restore-id>; usage: `restore status <restore-id>`",
        ));
    }

    let restore_id = parse_u64(&args[0], "restore-id must be an unsigned integer")?;

    let mut json_output = false;
    for arg in &args[1..] {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown restore status option; supported option is --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected restore status argument; supported option is --json",
                ));
            }
        }
    }

    let report = RestoreStatusReport {
        restore_id,
        state: RestoreState::ContractPreview,
        backup_id: 100,
        pitr_target_lsn: None,
        progress_percent: 0,
        wal_segments_replayed: 0,
        estimated_total_segments: 0,
        start_time: 0,
        elapsed_seconds: 0,
    };

    if json_output {
        print_restore_status_json(&report);
    } else {
        print_restore_status_human(&report);
    }

    Ok(())
}

fn print_restore_help() {
    println!("Andromeda restore administration commands");
    println!();
    println!("USAGE: andromeda-cli restore <SUBCOMMAND> | restore <backup-id> [OPTIONS]");
    println!();
    println!("FORMS:");
    println!("  restore <backup-id> --artifact <dir> --pitr-lsn <lsn> --dry-run");
    println!(
        "                                      Validate restore preflight from backup artifact"
    );
    println!("  restore <backup-id> --artifact <dir> --pitr-policy latest --dry-run");
    println!(
        "                                      Select the artifact WAL end through an explicit policy"
    );
    println!(
        "  restore verify <backup-id> --artifact <dir> (--pitr-lsn <lsn>|--pitr-policy latest)"
    );
    println!("                                      Verify artifact preflight and replay plan");
    println!("  restore status <restore-id>        Show restore progress and state");
    println!();
    println!("OPTIONS:");
    println!("  --artifact <dir>                   File-backed backup artifact root");
    println!("  --pitr-lsn <lsn>                   Target LSN for point-in-time recovery");
    println!("  --pitr-policy latest               Explicitly restore to artifact WAL end");
    println!("  --validation-policy <full|minimal> Restore validation policy (default: full)");
    println!(
        "  --dry-run                          Validate mutating restore contract without orchestration"
    );
    println!("  --json                            Emit diagnostic machine-readable JSON output");
    println!("  -h, --help                         Show this help message");
    println!();
    println!(
        "STATES: {}",
        RestoreState::ALL
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn print_restore_status_human(report: &RestoreStatusReport) {
    println!("Restore Status Report");
    println!("====================");
    println!("Restore ID: {}", report.restore_id);
    println!("Backup ID: {}", report.backup_id);
    println!("Contract Preview: true");
    println!("Durable Backend: false");
    println!("Durable State Loaded: false");
    println!("Requires Restore Orchestrator: true");
    println!("Runtime Mode: contract_preview/static_restore_orchestrator");
    println!("State: {}", report.state);
    if let Some(pitr_lsn) = report.pitr_target_lsn {
        println!("PITR Target LSN: {}", pitr_lsn);
    }
    println!("Progress: {}%", report.progress_percent);
    println!("Start Time: {}", report.start_time);
    println!(
        "WAL Segments: {} / {}",
        report.wal_segments_replayed, report.estimated_total_segments
    );
    println!("Elapsed: {}s", report.elapsed_seconds);
}

fn print_restore_start_json(outcome: &RestoreStartOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.restore.start.v1\",\"contract_preview\":{},\"durable_backend\":{},\"requires_restore_orchestrator\":{},\"dry_run\":{},\"would_restore\":{},\"restore_id\":{},\"backup_id\":{},\"artifact_path\":{},\"pitr_target_lsn\":{},\"pitr_policy\":{},\"validation_policy\":{},\"preflight_validated\":{},\"replay_segments\":{},\"message\":{}}}",
        outcome.contract_preview,
        outcome.durable_backend,
        outcome.requires_restore_orchestrator,
        outcome.dry_run,
        outcome.would_restore,
        outcome
            .restore_id
            .map(|restore_id| restore_id.to_string())
            .unwrap_or_else(|| "null".to_string()),
        outcome.backup_id,
        json_option_string(Some(outcome.artifact_path.as_str())),
        json_option_u64(outcome.pitr_target_lsn),
        json_option_string(outcome.pitr_policy.as_deref()),
        json_string(&outcome.validation_policy),
        outcome.preflight_validated,
        restore_replay_segments_json(&outcome.replay_segments),
        json_string(&outcome.message),
    );
}

fn print_restore_status_json(report: &RestoreStatusReport) {
    println!(
        "{{\"schema\":\"andromeda.cli.restore.status.v1\",\"contract_preview\":true,\"durable_backend\":false,\"durable_state_loaded\":false,\"runtime_mode\":\"contract_preview/static_restore_orchestrator\",\"requires_restore_orchestrator\":true,\"restore_id\":{},\"state\":{},\"backup_id\":{},\"pitr_target_lsn\":{},\"progress_percent\":{},\"wal_segments_replayed\":{},\"estimated_total_segments\":{},\"start_time\":{},\"elapsed_seconds\":{},\"message\":\"contract preview: durable restore status backend is not wired; showing static command contract only\"}}",
        report.restore_id,
        json_string(&report.state.to_string()),
        report.backup_id,
        json_option_u64(report.pitr_target_lsn),
        report.progress_percent,
        report.wal_segments_replayed,
        report.estimated_total_segments,
        report.start_time,
        report.elapsed_seconds,
    );
}

fn print_restore_verify_human(outcome: &RestoreVerifyOutcome) {
    println!("Restore Verify");
    println!("==============");
    println!("Backup ID: {}", outcome.backup_id);
    println!("Artifact Path: {}", outcome.artifact_path);
    println!("PITR Target LSN: {}", outcome.pitr_target_lsn);
    if let Some(policy) = &outcome.pitr_policy {
        println!("PITR Policy: {policy}");
    }
    println!("Validation Policy: {}", outcome.validation_policy);
    println!("Source Checkpoint LSN: {}", outcome.source_checkpoint_lsn);
    println!("Replay Segments:");
    for segment in &outcome.replay_segments {
        println!(
            "  - #{} segment_id={} lsn={}..={} contains_pitr={}",
            segment.sequence_index,
            segment.segment_id,
            segment.first_lsn,
            segment.last_lsn,
            segment.contains_pitr_target
        );
    }
    println!("{}", outcome.message);
}

fn print_restore_verify_json(outcome: &RestoreVerifyOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.restore.verify.v1\",\"durable_backend\":true,\"backup_id\":{},\"artifact_path\":{},\"pitr_target_lsn\":{},\"pitr_policy\":{},\"validation_policy\":{},\"source_checkpoint_lsn\":{},\"replay_segments\":{},\"message\":{}}}",
        outcome.backup_id,
        json_string(&outcome.artifact_path),
        outcome.pitr_target_lsn,
        json_option_string(outcome.pitr_policy.as_deref()),
        json_string(&outcome.validation_policy),
        outcome.source_checkpoint_lsn,
        restore_replay_segments_json(&outcome.replay_segments),
        json_string(&outcome.message),
    );
}

fn restore_replay_segments_json(segments: &[RestoreReplaySegmentOutput]) -> String {
    let entries = segments
        .iter()
        .map(|segment| {
            format!(
                "{{\"sequence_index\":{},\"segment_id\":{},\"first_lsn\":{},\"last_lsn\":{},\"contains_pitr_target\":{}}}",
                segment.sequence_index,
                segment.segment_id,
                segment.first_lsn,
                segment.last_lsn,
                segment.contains_pitr_target
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", entries)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
