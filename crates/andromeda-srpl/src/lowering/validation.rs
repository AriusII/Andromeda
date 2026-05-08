//! Facade-only source diagnostic helpers for the SRPL lowering pipeline.

use std::collections::BTreeSet;

use crate::ProcedureAst;

/// Validates that parameter names, result stream names, and column names inside
/// each result stream are all unique. Returns a [`crate::SrplDiagnostic`] on
/// the first duplicate found.
pub(super) fn validate_ast_names_for_diagnostics(
    ast: &ProcedureAst,
    source: &str,
) -> Result<(), crate::SrplDiagnostic> {
    let mut parameter_names = BTreeSet::new();
    for parameter in &ast.parameters {
        if !parameter_names.insert(parameter.name.value.as_str()) {
            let (line, column) = line_column(source, parameter.name.span.start);
            return Err(crate::SrplDiagnostic::new(
                crate::DiagnosticPhase::Binding,
                Some(parameter.name.span),
                format!(
                    "SRPL procedure input names must be unique; procedure {}, parameter {}, line {}, column {}",
                    ast.name.value.as_catalog_path(),
                    parameter.name.value,
                    line,
                    column,
                ),
            ));
        }
    }

    let mut result_names = BTreeSet::new();
    for result in &ast.results {
        if !result_names.insert(result.name.value.as_str()) {
            let (line, column) = line_column(source, result.name.span.start);
            return Err(crate::SrplDiagnostic::new(
                crate::DiagnosticPhase::Binding,
                Some(result.name.span),
                format!(
                    "SRPL result stream names must be unique; procedure {}, result stream {}, line {}, column {}",
                    ast.name.value.as_catalog_path(),
                    result.name.value,
                    line,
                    column,
                ),
            ));
        }

        let mut column_names = BTreeSet::new();
        for column in &result.columns {
            if !column_names.insert(column.name.value.as_str()) {
                let (line, column_number) = line_column(source, column.name.span.start);
                return Err(crate::SrplDiagnostic::new(
                    crate::DiagnosticPhase::Binding,
                    Some(column.name.span),
                    format!(
                        "SRPL result column names must be unique; procedure {}, result stream {}, column {}, line {}, column {}",
                        ast.name.value.as_catalog_path(),
                        result.name.value,
                        column.name.value,
                        line,
                        column_number,
                    ),
                ));
            }
        }
    }

    Ok(())
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
