#![forbid(unsafe_code)]

//! SRPL syntax model.
//!
//! This crate owns AST data shapes only. It does not lex, parse, bind, lower,
//! execute, or read catalog storage.

mod ast;

pub use andromeda_srpl_cardinality::Cardinality;
pub use andromeda_srpl_diagnostics::SourceSpan;
pub use ast::{
    BusinessOperationAst, BusinessOperationKindAst, FieldAst, ProcedureAst, ProcedureBodyAst,
    ResultStreamAst, Spanned,
};
