//! HADR administration command facade.

use andromeda_core::AndromedaResult;

/// Parses and executes HADR administration subcommands.
pub fn run_hadr_command(args: &[String]) -> AndromedaResult<()> {
    crate::hadr::run_hadr_command(args)
}
