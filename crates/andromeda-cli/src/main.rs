#![forbid(unsafe_code)]

//! Andromeda CLI binary.
//!
//! Command-line interface for Andromeda database engine.

use andromeda_cli::dispatch_command;
use andromeda_error::AndromedaResult;

fn main() -> AndromedaResult<()> {
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    dispatch_command(&args)
}
