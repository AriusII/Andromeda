use crate::diagnostic_json::{JSON_FLAG, parse_json_flag};
use crate::error::cli_error;
use andromeda_core::AndromedaResult;

pub(super) fn has_json_option(args: &[String]) -> bool {
    args.iter().any(|arg| arg == JSON_FLAG)
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

pub(super) fn has_dry_run_option(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--dry-run")
}

pub(super) fn parse_node_id_arg(args: &[String], missing_message: &'static str) -> AndromedaResult<u64> {
    let raw = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .ok_or_else(|| cli_error(missing_message))?;
    let node_id: u64 = raw
        .parse()
        .map_err(|_| cli_error("node-id must be an unsigned integer"))?;
    if node_id == 0 {
        return Err(cli_error("node-id must not be zero"));
    }
    Ok(node_id)
}

pub(super) fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].as_str())
}
