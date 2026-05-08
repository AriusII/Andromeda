#![forbid(unsafe_code)]

//! SRPL bound-input lowering.
//!
//! This crate owns the bounded lowering behavior that can sit below the
//! historical `andromeda-srpl` facade without depending on that facade. Source
//! parsing, local facade binding wrappers, optimizer entry points, runtime
//! execution, and catalog publication remain outside this crate.
//!
//! Dependency direction:
//! - consume typed AST/body input and lower it into typed IR;
//! - avoid parser ownership, catalog storage, execution, storage, transaction,
//!   WAL, transport, benchmark, analytics, GPU, and application-surface dependencies.

mod contract;
mod pipeline;

pub use contract::lower_ir_to_contract_candidate;
pub use pipeline::{BoundProcedureLoweringInput, lower_body_ast, lower_bound_procedure};
