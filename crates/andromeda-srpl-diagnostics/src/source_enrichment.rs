//! Source-oriented diagnostic enrichment helpers.

use crate::SrplDiagnostic;

pub fn enrich_source_diagnostic(
    source: &str,
    diagnostic: SrplDiagnostic,
    procedure_name: Option<&str>,
) -> SrplDiagnostic {
    let Some(span) = diagnostic.location else {
        return diagnostic;
    };
    let (line, column) = line_column(source, span.start);
    let mut message = diagnostic.message;
    if let Some(name) = procedure_name {
        message.push_str(&format!("; procedure {name}"));
    }
    message.push_str(&format!("; line {line}, column {column}"));
    SrplDiagnostic::new(diagnostic.phase, Some(span), message)
}

pub fn line_column(source: &str, offset: usize) -> (usize, usize) {
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
