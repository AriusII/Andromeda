use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64, parse_usize};
use andromeda_error::AndromedaResult;

#[derive(Debug, Clone)]
pub(super) struct BackupStartOptions {
    pub(super) backup_id: Option<u64>,
    pub(super) backup_type: String,
    pub(super) destination: Option<String>,
    pub(super) artifact_dir: Option<String>,
    pub(super) runtime: bool,
    pub(super) json_output: bool,
    pub(super) dry_run: bool,
}

#[derive(Debug, Clone)]
pub(super) struct BackupStatusOptions {
    pub(super) backup_id: u64,
    pub(super) artifact_dir: Option<String>,
    pub(super) json_output: bool,
}

#[derive(Debug, Clone)]
pub(super) struct BackupListOptions {
    pub(super) limit: usize,
    pub(super) artifact_dir: Option<String>,
    pub(super) json_output: bool,
}

#[derive(Debug, Clone)]
pub(super) struct BackupVerifyOptions {
    pub(super) backup_id: u64,
    pub(super) artifact_dir: String,
    pub(super) json_output: bool,
}

#[derive(Debug, Clone)]
pub(super) struct BackupCancelOptions {
    pub(super) backup_id: u64,
    pub(super) artifact_dir: Option<String>,
    pub(super) json_output: bool,
    pub(super) dry_run: bool,
}

pub(super) fn parse_backup_start(args: &[String]) -> AndromedaResult<BackupStartOptions> {
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
            },
            "--artifact-dir" => {
                artifact_dir = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--artifact-dir requires a directory path",
                    )?
                    .to_string(),
                );
            },
            "--destination" => {
                destination = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--destination requires a path argument",
                    )?
                    .to_string(),
                );
            },
            "--dry-run" => dry_run = true,
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup start option; supported options are --incremental, --destination, --artifact-dir, --backup-id, --runtime, --dry-run, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected backup start argument; supported options are --incremental, --destination, --artifact-dir, --backup-id, --runtime, --dry-run, and --json",
                ));
            },
        }
        i += 1;
    }

    Ok(BackupStartOptions {
        backup_id,
        backup_type: if incremental {
            "incremental".to_string()
        } else {
            "full".to_string()
        },
        destination,
        artifact_dir,
        runtime,
        json_output,
        dry_run,
    })
}

pub(super) fn parse_backup_status(args: &[String]) -> AndromedaResult<BackupStatusOptions> {
    if args.is_empty() {
        return Err(cli_error(
            "backup status requires <backup-id>; usage: `backup status <backup-id>`",
        ));
    }

    let backup_id = parse_backup_id(&args[0])?;
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
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup status option; supported options are --runtime, --artifact-dir, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected backup status argument; supported options are --runtime, --artifact-dir, and --json",
                ));
            },
        }
        i += 1;
    }

    if runtime && artifact_dir.is_none() {
        return Err(cli_error(
            "backup status --runtime requires --artifact-dir <dir> for file-backed state",
        ));
    }

    Ok(BackupStatusOptions {
        backup_id,
        artifact_dir,
        json_output,
    })
}

pub(super) fn parse_backup_list(args: &[String]) -> AndromedaResult<BackupListOptions> {
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
            },
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
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup list option; supported options are --runtime, --artifact-dir, --limit, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected backup list argument; supported options are --runtime, --artifact-dir, --limit, and --json",
                ));
            },
        }
        i += 1;
    }

    if runtime && artifact_dir.is_none() {
        return Err(cli_error(
            "backup list --runtime requires --artifact-dir <dir> for file-backed state",
        ));
    }

    Ok(BackupListOptions {
        limit,
        artifact_dir,
        json_output,
    })
}

pub(super) fn parse_backup_verify(args: &[String]) -> AndromedaResult<BackupVerifyOptions> {
    if args.is_empty() {
        return Err(cli_error(
            "backup verify requires <backup-id>; usage: `backup verify <backup-id> --artifact-dir <dir>`",
        ));
    }

    let backup_id = parse_backup_id(&args[0])?;
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
            },
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup verify option; supported options are --runtime, --artifact-dir, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected backup verify argument; supported options are --runtime, --artifact-dir, and --json",
                ));
            },
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

    Ok(BackupVerifyOptions {
        backup_id,
        artifact_dir,
        json_output,
    })
}

pub(super) fn parse_backup_cancel(args: &[String]) -> AndromedaResult<BackupCancelOptions> {
    if args.is_empty() {
        return Err(cli_error(
            "backup cancel requires <backup-id>; usage: `backup cancel <backup-id> [--artifact-dir <dir>|--dry-run]`",
        ));
    }

    let backup_id = parse_backup_id(&args[0])?;
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
            },
            "--dry-run" => dry_run = true,
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown backup cancel option; supported options are --runtime, --artifact-dir, --dry-run, and --json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected backup cancel argument; supported options are --runtime, --artifact-dir, --dry-run, and --json",
                ));
            },
        }
        i += 1;
    }

    if runtime && artifact_dir.is_none() {
        return Err(cli_error(
            "backup cancel --runtime requires --artifact-dir <dir> for file-backed state",
        ));
    }

    Ok(BackupCancelOptions {
        backup_id,
        artifact_dir,
        json_output,
        dry_run,
    })
}

fn parse_backup_id(value: &str) -> AndromedaResult<u64> {
    let backup_id = parse_u64(value, "backup-id must be an unsigned integer")?;
    if backup_id == 0 {
        return Err(cli_error("backup-id must be greater than zero"));
    }
    Ok(backup_id)
}
