use andromeda_catalog::QualifiedName;
use andromeda_error::{AndromedaError, AndromedaErrorKind};

use crate::{DiagnosticPhase, SrplDiagnostic};

/// A diagnostic emitted while compiling one SRPL source or validating the
/// resulting DefinitionBatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDiagnostic {
    pub source_index: Option<usize>,
    pub procedure_name: Option<QualifiedName>,
    pub diagnostic: SrplDiagnostic,
}

impl SrplDefinitionBatchDiagnostic {
    pub fn source(source_index: usize, diagnostic: SrplDiagnostic) -> Self {
        Self {
            source_index: Some(source_index),
            procedure_name: None,
            diagnostic,
        }
    }

    pub fn source_for_procedure(
        source_index: usize,
        procedure_name: QualifiedName,
        diagnostic: SrplDiagnostic,
    ) -> Self {
        Self {
            source_index: Some(source_index),
            procedure_name: Some(procedure_name),
            diagnostic,
        }
    }

    pub fn batch(message: impl Into<String>) -> Self {
        Self {
            source_index: None,
            procedure_name: None,
            diagnostic: SrplDiagnostic::new(DiagnosticPhase::SemanticValidation, None, message),
        }
    }
}

/// Deterministic rejection for an SRPL DefinitionBatch source dry-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplDefinitionBatchDryRunError {
    pub diagnostics: Vec<SrplDefinitionBatchDiagnostic>,
}

impl SrplDefinitionBatchDryRunError {
    pub fn new(diagnostics: Vec<SrplDefinitionBatchDiagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Srpl, self.to_string())
    }
}

impl std::fmt::Display for SrplDefinitionBatchDryRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SRPL DefinitionBatch dry-run rejected {} diagnostic(s)",
            self.diagnostics.len()
        )?;
        for diagnostic in &self.diagnostics {
            let source = diagnostic
                .source_index
                .map(|index| format!("source[{index}]"))
                .unwrap_or_else(|| "batch".to_string());
            let procedure = diagnostic
                .procedure_name
                .as_ref()
                .map(|name| format!(" {}", name.as_catalog_path()))
                .unwrap_or_default();
            write!(
                f,
                "; {source}{procedure}: {}",
                diagnostic.diagnostic.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for SrplDefinitionBatchDryRunError {}
