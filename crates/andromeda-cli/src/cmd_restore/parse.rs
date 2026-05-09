use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_core::AndromedaResult;
use andromeda_restore::RestoreValidationPolicy;

#[derive(Debug, Clone)]
pub(super) struct RestoreStartOptions {
    pub(super) backup_id: u64,
    pub(super) artifact_path: String,
    pub(super) pitr_target_lsn: Option<u64>,
    pub(super) pitr_policy: Option<RestorePitrPolicy>,
    pub(super) validation_policy: RestoreValidationPolicy,
    pub(super) json_output: bool,
    pub(super) dry_run: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RestoreVerifyOptions {
    pub(super) backup_id: u64,
    pub(super) artifact_path: String,
    pub(super) pitr_target_lsn: Option<u64>,
    pub(super) pitr_policy: Option<RestorePitrPolicy>,
    pub(super) validation_policy: RestoreValidationPolicy,
    pub(super) json_output: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RestoreStatusOptions {
    pub(super) restore_id: u64,
    pub(super) json_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RestorePitrPolicy {
    Latest,
}

pub(super) fn parse_restore_start(args: &[String]) -> AndromedaResult<RestoreStartOptions> {
    if args.is_empty() {
        return Err(cli_error(
            "restore requires <backup-id>; usage: `restore <backup-id> [--pitr-lsn <lsn>]`",
        ));
    }

    let backup_id = parse_backup_id(&args[0])?;
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
            },
            "--pitr-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--pitr-policy requires a policy value",
                )?;
                pitr_policy = Some(parse_restore_pitr_policy(value)?);
            },
            "--validation-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--validation-policy requires full or minimal",
                )?;
                validation_policy = parse_restore_validation_policy(value)?;
            },
            "--artifact" | "--artifact-path" => {
                artifact_path = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact requires a backup artifact path",
                    )?
                    .to_string(),
                );
            },
            "--dry-run" => dry_run = true,
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown restore option; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, --dry-run, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected restore argument; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, --dry-run, and --json",
                ));
            },
        }
        i += 1;
    }

    validate_restore_target("restore", pitr_target_lsn, pitr_policy)?;

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

    Ok(RestoreStartOptions {
        backup_id,
        artifact_path,
        pitr_target_lsn,
        pitr_policy,
        validation_policy,
        json_output,
        dry_run,
    })
}

pub(super) fn parse_restore_verify(args: &[String]) -> AndromedaResult<RestoreVerifyOptions> {
    if args.is_empty() {
        return Err(cli_error(
            "restore verify requires <backup-id>; usage: `restore verify <backup-id> --artifact <dir> (--pitr-lsn <lsn>|--pitr-policy latest)`",
        ));
    }

    let backup_id = parse_backup_id(&args[0])?;
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
            },
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
            },
            "--pitr-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--pitr-policy requires a policy value",
                )?;
                pitr_policy = Some(parse_restore_pitr_policy(value)?);
            },
            "--validation-policy" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--validation-policy requires full or minimal",
                )?;
                validation_policy = parse_restore_validation_policy(value)?;
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown restore verify option; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected restore verify argument; supported options are --artifact, --pitr-lsn, --pitr-policy, --validation-policy, and --json",
                ));
            },
        }
        i += 1;
    }

    validate_restore_target("restore verify", pitr_target_lsn, pitr_policy)?;
    let artifact_path =
        artifact_path.ok_or_else(|| cli_error("restore verify requires --artifact <dir>"))?;

    Ok(RestoreVerifyOptions {
        backup_id,
        artifact_path,
        pitr_target_lsn,
        pitr_policy,
        validation_policy,
        json_output,
    })
}

pub(super) fn parse_restore_status(args: &[String]) -> AndromedaResult<RestoreStatusOptions> {
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
            },
            _ => {
                return Err(cli_error(
                    "unexpected restore status argument; supported option is --json",
                ));
            },
        }
    }

    Ok(RestoreStatusOptions {
        restore_id,
        json_output,
    })
}

pub(super) fn restore_pitr_policy_str(policy: RestorePitrPolicy) -> &'static str {
    match policy {
        RestorePitrPolicy::Latest => "latest",
    }
}

pub(super) fn restore_validation_policy_str(policy: RestoreValidationPolicy) -> &'static str {
    match policy {
        RestoreValidationPolicy::Full => "full",
        RestoreValidationPolicy::Minimal => "minimal",
    }
}

fn validate_restore_target(
    command_name: &str,
    pitr_target_lsn: Option<u64>,
    pitr_policy: Option<RestorePitrPolicy>,
) -> AndromedaResult<()> {
    if pitr_target_lsn.is_some() && pitr_policy.is_some() {
        return Err(cli_error(format!(
            "{command_name} requires either --pitr-lsn or --pitr-policy, not both"
        )));
    }
    if pitr_target_lsn.is_none() && pitr_policy.is_none() {
        return Err(cli_error(format!(
            "{command_name} requires an explicit --pitr-lsn <lsn> or --pitr-policy latest"
        )));
    }
    if matches!(pitr_target_lsn, Some(0)) {
        return Err(cli_error("--pitr-lsn must be greater than zero"));
    }
    Ok(())
}

fn parse_backup_id(value: &str) -> AndromedaResult<u64> {
    let backup_id = parse_u64(value, "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }
    Ok(backup_id)
}

fn parse_restore_pitr_policy(value: &str) -> AndromedaResult<RestorePitrPolicy> {
    match value {
        "latest" => Ok(RestorePitrPolicy::Latest),
        _ => Err(cli_error("unknown restore PITR policy; expected latest")),
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
