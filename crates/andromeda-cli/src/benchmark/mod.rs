//! Benchmark administration commands.
//!
//! This command is an operations/diagnostic surface for bounded benchmark
//! orchestration. It does not introduce an application runtime path or alternate
//! wire semantics. Structured output, when requested, is diagnostic only.

mod command;
mod crud;
mod error;
mod output;
mod parse;

pub use command::run_benchmark_command;
