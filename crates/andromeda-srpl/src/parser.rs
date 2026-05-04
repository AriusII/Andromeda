//! Public parser facade for the bounded SRPL compiler slice.

mod core;
mod helpers;
mod statements;
mod types;

use crate::{lex, ProcedureAst, SrplDiagnostic};

use self::core::Parser;

pub fn parse_procedure_signature(input: &str) -> Result<ProcedureAst, SrplDiagnostic> {
    let tokens = lex(input)?;
    Parser::new(tokens, input.len()).parse_procedure()
}
