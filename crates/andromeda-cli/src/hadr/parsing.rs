use std::path::PathBuf;

use crate::diagnostic_json::JSON_FLAG;
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64, parse_usize};
use andromeda_core::AndromedaResult;

const MEMBERSHIP_STORE_FLAG: &str = "--membership-store";
const STATE_DIR_FLAG: &str = "--state-dir";
const MEMBERSHIP_STORE_FILE_NAME: &str = "hadr-membership.bin";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HadrReadOptions {
    pub(super) json_output: bool,
    pub(super) membership_store: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FailoverPrepareOptions {
    pub(super) json_output: bool,
    pub(super) witness_check: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromoteOptions {
    pub(super) json_output: bool,
    pub(super) dry_run: bool,
    pub(super) apply: bool,
    pub(super) membership_store: Option<PathBuf>,
    pub(super) promotion_audit_log: Option<PathBuf>,
    pub(super) candidate_lsn: Option<u64>,
    pub(super) audit_lsn: Option<u64>,
    pub(super) commit_quorum: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DemoteOptions {
    pub(super) json_output: bool,
    pub(super) force: bool,
    pub(super) dry_run: bool,
}

pub(super) fn parse_status_options(args: &[String]) -> AndromedaResult<HadrReadOptions> {
    parse_read_options(
        args,
        "hadr status",
        "supported options are --membership-store, --state-dir, and --json",
    )
}

pub(super) fn parse_quorum_options(args: &[String]) -> AndromedaResult<HadrReadOptions> {
    parse_read_options(
        args,
        "hadr quorum",
        "supported options are --membership-store, --state-dir, and --json",
    )
}

pub(super) fn parse_membership_store_option(
    args: &[String],
    index: &mut usize,
    membership_store: &mut Option<PathBuf>,
) -> AndromedaResult<bool> {
    match args[*index].as_str() {
        MEMBERSHIP_STORE_FLAG => {
            let value = next_option_value_rejecting_flag(
                args,
                index,
                "--membership-store requires a file path",
            )?;
            set_membership_store(
                membership_store,
                PathBuf::from(value),
                "--membership-store and --state-dir cannot be provided together",
            )?;
            Ok(true)
        }
        STATE_DIR_FLAG => {
            let value =
                next_option_value_rejecting_flag(args, index, "--state-dir requires a directory")?;
            set_membership_store(
                membership_store,
                PathBuf::from(value).join(MEMBERSHIP_STORE_FILE_NAME),
                "--membership-store and --state-dir cannot be provided together",
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn parse_read_options(
    args: &[String],
    command: &str,
    supported_options: &'static str,
) -> AndromedaResult<HadrReadOptions> {
    let mut json_output = false;
    let mut membership_store = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            MEMBERSHIP_STORE_FLAG | STATE_DIR_FLAG => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown {command} option; {supported_options}"
                )));
            }
            _ => {
                return Err(cli_error(format!(
                    "unexpected {command} argument; {supported_options}"
                )));
            }
        }
        index += 1;
    }

    Ok(HadrReadOptions {
        json_output,
        membership_store,
    })
}

fn set_membership_store(
    target: &mut Option<PathBuf>,
    path: PathBuf,
    duplicate_message: &'static str,
) -> AndromedaResult<()> {
    if target.is_some() {
        return Err(cli_error(duplicate_message));
    }
    *target = Some(path);
    Ok(())
}

pub(super) fn parse_promote_options(args: &[String]) -> AndromedaResult<PromoteOptions> {
    let mut json_output = false;
    let mut dry_run = false;
    let mut apply = false;
    let mut membership_store = None;
    let mut promotion_audit_log = None;
    let mut candidate_lsn = None;
    let mut audit_lsn = None;
    let mut commit_quorum = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => json_output = true,
            "--apply" => apply = true,
            "--dry-run" => dry_run = true,
            MEMBERSHIP_STORE_FLAG | STATE_DIR_FLAG => {
                parse_membership_store_option(args, &mut index, &mut membership_store)?;
            }
            "--promotion-audit-log" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--promotion-audit-log requires a file path",
                )?;
                set_membership_store(
                    &mut promotion_audit_log,
                    PathBuf::from(value),
                    "--promotion-audit-log cannot be provided more than once",
                )?;
            }
            "--candidate-lsn" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--candidate-lsn requires an unsigned integer",
                )?;
                set_u64_option(
                    &mut candidate_lsn,
                    parse_u64(value, "--candidate-lsn expects an unsigned integer")?,
                    "--candidate-lsn cannot be provided more than once",
                )?;
            }
            "--audit-lsn" | "--primary-durable-lsn" => {
                let option = args[index].as_str();
                let missing_message = if option == "--audit-lsn" {
                    "--audit-lsn requires an unsigned integer"
                } else {
                    "--primary-durable-lsn requires an unsigned integer"
                };
                let parse_message = if option == "--audit-lsn" {
                    "--audit-lsn expects an unsigned integer"
                } else {
                    "--primary-durable-lsn expects an unsigned integer"
                };
                let value = next_option_value_rejecting_flag(args, &mut index, missing_message)?;
                set_u64_option(
                    &mut audit_lsn,
                    parse_u64(value, parse_message)?,
                    if option == "--audit-lsn" {
                        "--audit-lsn cannot be provided more than once"
                    } else {
                        "--primary-durable-lsn cannot be combined with --audit-lsn"
                    },
                )?;
            }
            "--commit-quorum" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut index,
                    "--commit-quorum requires an unsigned integer",
                )?;
                set_usize_option(
                    &mut commit_quorum,
                    parse_usize(value, "--commit-quorum expects an unsigned integer")?,
                    "--commit-quorum cannot be provided more than once",
                )?;
            }
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr promote option; supported options are --apply, --dry-run, --membership-store, --state-dir, --candidate-lsn, --audit-lsn, --primary-durable-lsn, --commit-quorum, --promotion-audit-log, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected hadr promote argument; supported options are --apply, --dry-run, --membership-store, --state-dir, --candidate-lsn, --audit-lsn, --primary-durable-lsn, --commit-quorum, --promotion-audit-log, and --json",
                ));
            }
        }
        index += 1;
    }
    if dry_run && apply {
        return Err(cli_error(
            "hadr promote accepts only one of --apply or --dry-run",
        ));
    }
    Ok(PromoteOptions {
        json_output,
        dry_run,
        apply,
        membership_store,
        promotion_audit_log,
        candidate_lsn,
        audit_lsn,
        commit_quorum,
    })
}

fn set_u64_option(
    target: &mut Option<u64>,
    value: u64,
    duplicate_message: &'static str,
) -> AndromedaResult<()> {
    if target.is_some() {
        return Err(cli_error(duplicate_message));
    }
    *target = Some(value);
    Ok(())
}

fn set_usize_option(
    target: &mut Option<usize>,
    value: usize,
    duplicate_message: &'static str,
) -> AndromedaResult<()> {
    if target.is_some() {
        return Err(cli_error(duplicate_message));
    }
    *target = Some(value);
    Ok(())
}

pub(super) fn parse_demote_options(args: &[String]) -> AndromedaResult<DemoteOptions> {
    let mut json_output = false;
    let mut force = false;
    let mut dry_run = false;
    for arg in args {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            "--force" => force = true,
            "--dry-run" => dry_run = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr demote option; supported options are --force, --dry-run, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected hadr demote argument; supported options are --force, --dry-run, and --json",
                ));
            }
        }
    }
    Ok(DemoteOptions {
        json_output,
        force,
        dry_run,
    })
}

pub(super) fn parse_failover_prepare_options(
    args: &[String],
) -> AndromedaResult<FailoverPrepareOptions> {
    let mut json_output = false;
    let mut witness_check = false;
    for arg in args {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            "--witness-check" => witness_check = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown hadr failover-prepare option; supported options are --witness-check and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected hadr failover-prepare argument; supported options are --witness-check and --json",
                ));
            }
        }
    }

    Ok(FailoverPrepareOptions {
        json_output,
        witness_check,
    })
}
