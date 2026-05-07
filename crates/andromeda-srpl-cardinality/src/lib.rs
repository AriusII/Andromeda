#![forbid(unsafe_code)]

//! SRPL result cardinality policy.
//!
//! This crate owns cardinality semantics shared by parser, IR, and execution
//! adapters without depending on catalog storage or runtime execution.

mod cardinality;

pub use cardinality::*;
