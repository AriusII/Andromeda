#![forbid(unsafe_code)]

//! SRPL compiler facade.
//!
//! The crate keeps its historical root-level re-exports for compatibility,
//! while also exposing professionalized module boundaries:
//! - [`procedure_compiler`] owns source-to-AST binding and IR lowering entry points.
//! - [`procedure_model`] owns AST, contract, cardinality, and IR data shapes.
//! - [`SrplDiagnostic`] and [`source_location`] own source spans and validation diagnostics.

mod ast;
mod binder;
mod cardinality;
pub mod definition_batch_bridge;
mod diagnostics;
pub mod execution_adapter;
mod identifier;
pub mod interpreter;
mod ir;
mod lexer;
mod lowering;
pub mod optimizer;
mod parser;
pub mod procedure_compiler;
pub mod procedure_model;
pub mod procedure_resolver;
mod signature;
pub mod source_location;

pub use definition_batch_bridge::*;
pub use diagnostics::*;
pub use procedure_compiler::*;
pub use procedure_model::*;
pub use source_location::*;
