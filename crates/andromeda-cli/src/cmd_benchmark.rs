//! Benchmark administration command facade.

use andromeda_core::AndromedaResult;

/// Parses and executes benchmark administration subcommands.
pub fn run_benchmark_command(args: &[String]) -> AndromedaResult<()> {
    crate::benchmark::run_benchmark_command(args)
}
