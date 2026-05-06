#![forbid(unsafe_code)]

mod args_parser;
mod audit;
mod benchmark;
mod cmd_backup;
mod cmd_catalog;
mod cmd_protocol;
mod cmd_recovery;
mod cmd_restore;
mod cmd_vertical;
mod diagnostic_json;
mod error_mod;
mod hadr;
mod parse;
mod proto_helpers;

pub mod error {
    pub use crate::error_mod::*;
}

pub mod args {
    pub use crate::args_parser::*;
}

pub mod cmd;

pub mod output {}

pub use cmd::dispatch_command;
pub use error::{cli_error, protocol_error};
