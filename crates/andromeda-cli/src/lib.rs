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

mod error {
    pub(crate) use crate::error_mod::*;
}

mod args {
    pub(crate) use crate::args_parser::*;
}

mod cmd;

pub use cmd::dispatch_command;
