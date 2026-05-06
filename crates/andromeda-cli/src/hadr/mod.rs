mod demote;
mod failover_prepare;
mod node;
mod output;
mod parsing;
mod promote;
mod quorum;
mod runtime;
mod status;
mod types;

use crate::error::cli_error;
use andromeda_core::AndromedaResult;

pub fn run_hadr_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("status") => status::run_hadr_status(&args[1..]),
        Some("node") => node::run_hadr_node(&args[1..]),
        Some("promote") => promote::run_hadr_promote(&args[1..]),
        Some("demote") => demote::run_hadr_demote(&args[1..]),
        Some("failover-prepare") => failover_prepare::run_hadr_failover_prepare(&args[1..]),
        Some("quorum") => quorum::run_hadr_quorum(&args[1..]),
        Some("-h" | "--help" | "help") => {
            output::print_hadr_help();
            Ok(())
        }
        Some(_) => Err(cli_error(
            "unknown hadr subcommand; run `andromeda-cli hadr --help`",
        )),
        None => {
            output::print_hadr_help();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
