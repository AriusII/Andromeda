#![forbid(unsafe_code)]

//! SRPL semantic IR and procedure signature model.
//!
//! This crate owns bounded SRPL IR data shapes and validation. It does not
//! parse source text, execute runtime behavior, or depend on catalog storage.

mod identifier;
mod ir;
mod signature;

pub use andromeda_srpl_cardinality::Cardinality;
pub use ir::*;
pub use signature::*;
