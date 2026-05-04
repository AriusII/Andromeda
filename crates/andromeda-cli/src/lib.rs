//! Andromeda CLI library.
//!
//! This library provides the command-line interface for Andromeda, with modular
//! command, argument parsing, output formatting, and error handling.

#![forbid(unsafe_code)]

// Module files
mod args_parser;
mod cmd_protocol;
mod cmd_recovery;
mod cmd_vertical;
mod error_mod;
mod proto_helpers;

// Public modules with re-exports
pub mod error {
    pub use crate::error_mod::*;
}

pub mod args {
    pub use crate::args_parser::*;
}

pub mod cmd;

pub mod output {
    // Placeholder for output formatting
}

pub use cmd::dispatch_command;
pub use error::{cli_error, protocol_error};
