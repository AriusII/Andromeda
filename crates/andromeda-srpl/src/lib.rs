#![forbid(unsafe_code)]

//! SRPL compiler facade.
//!
//! The crate keeps its historical root-level re-exports for compatibility,
//! while also exposing professionalized module boundaries:
//! - [`compiler`] owns source-to-AST binding and IR lowering entry points.
//! - [`model`] owns AST, contract, cardinality, and IR data shapes.
//! - [`diagnostics`] and [`source`] own source spans and validation diagnostics.

mod ast;
mod binder;
mod cardinality;
pub mod compiler;
pub mod diagnostics;
mod ir;
mod lexer;
mod lowering;
pub mod model;
mod parser;
mod signature;
pub mod source;

pub use compiler::*;
pub use diagnostics::*;
pub use model::*;
pub use source::*;
