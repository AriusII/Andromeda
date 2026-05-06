use crate::diagnostic_json::{JSON_FLAG, parse_json_flag};
use crate::error::cli_error;
use andromeda_core::AndromedaResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FailoverPrepareOptions {
    pub(super) json_output: bool,
    pub(super) witness_check: bool,
}

pub(super) fn parse_status_json_option(args: &[String]) -> AndromedaResult<bool> {
    parse_json_flag(args, "hadr status")
}

pub(super) fn parse_quorum_json_option(args: &[String]) -> AndromedaResult<bool> {
    parse_json_flag(args, "hadr quorum")
}

pub(super) fn parse_promote_json_option(args: &[String]) -> AndromedaResult<bool> {
    parse_json_flag(args, "hadr promote")
}

pub(super) fn parse_demote_options(args: &[String]) -> AndromedaResult<(bool, bool)> {
    let mut json_output = false;
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            "--force" => force = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown hadr demote option: {opt}; supported options are --force and --json"
                )));
            }
            value => {
                return Err(cli_error(format!(
                    "unexpected hadr demote argument: {value}; supported options are --force and --json"
                )));
            }
        }
    }
    Ok((json_output, force))
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
                return Err(cli_error(format!(
                    "unknown hadr failover-prepare option: {opt}"
                )));
            }
            value => {
                return Err(cli_error(format!(
                    "unexpected hadr failover-prepare argument: {value}"
                )));
            }
        }
    }

    Ok(FailoverPrepareOptions {
        json_output,
        witness_check,
    })
}
