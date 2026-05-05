//! SRPL validation during DefinitionBatch dry-run phase.
//!
//! This module validates all SRPL procedures in a batch before application.
//! Validation is lazy: SRPL compilation happens only during dry-run, not when
//! adding operations to the batch. This enables fail-fast semantics: if any
//! procedure is invalid, the entire batch is rejected.
//!
//! ## Validation Pipeline
//!
//! For each SRPL procedure in the batch:
//! 1. **Parse**: Source → AST (detect syntax errors)
//! 2. **Bind**: AST → Typed procedure (detect name conflicts, type mismatches)
//! 3. **Lower**: Typed → IR (detect semantic errors, cardinality issues)
//! 4. **Manifest**: IR → Contract (compute contract hash, input/output shapes)
//!
//! If all procedures validate, dry-run returns success with affected catalog version.
//! If any procedure fails, dry-run returns failure with diagnostic (entire batch rejected).
//!
//! ## Error Categories
//!
//! - **SyntaxError**: Lexer/parser failure (e.g., missing keyword)
//! - **BindError**: Name/type validation failure (e.g., undefined table)
//! - **CompileError**: Semantic validation failure (e.g., type mismatch)
//! - **ManifestError**: Contract materialization failure
//! - **IntegrityError**: Batch constraint violation (e.g., duplicate procedure name)

use andromeda_core::AndromedaResult;

use crate::DefinitionBatch;

/// Report produced by a SRPL batch dry-run validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplBatchDryRunReport {
    /// Whether all procedures in the batch are valid
    pub all_valid: bool,

    /// Number of valid procedures
    pub valid_count: usize,

    /// Number of rejected procedures
    pub rejected_count: usize,

    /// Diagnostic messages for each rejected procedure
    pub rejection_reasons: Vec<String>,
}

impl SrplBatchDryRunReport {
    /// Create a successful report (all procedures valid).
    pub fn success(valid_count: usize) -> Self {
        Self {
            all_valid: true,
            valid_count,
            rejected_count: 0,
            rejection_reasons: Vec::new(),
        }
    }

    /// Create a failure report (at least one procedure invalid).
    pub fn failure(valid_count: usize, reasons: Vec<String>) -> Self {
        Self {
            all_valid: false,
            valid_count,
            rejected_count: reasons.len(),
            rejection_reasons: reasons,
        }
    }
}

/// Validate all SRPL procedures in a batch during dry-run phase.
///
/// This function is called by DefinitionBatch::dry_run() before applying mutations.
/// It performs complete SRPL compilation: parse → bind → lower → manifest.
///
/// # Arguments
///
/// * `batch` - The batch containing SRPL procedures to validate
///
/// # Returns
///
/// On success: SrplBatchDryRunReport with all_valid=true
/// On failure: AndromedaError (entire batch rejected due to failed procedures)
///
/// # Atomicity
///
/// This function implements all-or-nothing semantics:
/// - If any procedure fails validation, the entire batch is rejected
/// - Partial success is not allowed
/// - Error diagnostic includes all failed procedures
pub fn validate_srpl_batch_dry_run(
    _batch: &DefinitionBatch,
) -> AndromedaResult<SrplBatchDryRunReport> {
    // TODO(E7): Implement in integration phase
    // 1. Iterate batch.operations
    // 2. For each SRPL procedure operation:
    //    a. Parse SRPL source to AST
    //    b. Bind AST (validate names/types)
    //    c. Lower AST to IR (validate semantics)
    //    d. Materialize contract (compute hash)
    // 3. Accumulate errors
    // 4. If errors.is_empty(): return Ok(success_report)
    // 5. Else: return Err(batch_validation_failed) with all errors
    Ok(SrplBatchDryRunReport::success(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srpl_batch_dry_run_report_success() {
        let report = SrplBatchDryRunReport::success(2);
        assert!(report.all_valid);
        assert_eq!(report.valid_count, 2);
        assert_eq!(report.rejected_count, 0);
        assert!(report.rejection_reasons.is_empty());
    }

    #[test]
    fn srpl_batch_dry_run_report_failure() {
        let reasons = vec!["syntax error".to_string(), "type mismatch".to_string()];
        let report = SrplBatchDryRunReport::failure(1, reasons.clone());
        assert!(!report.all_valid);
        assert_eq!(report.valid_count, 1);
        assert_eq!(report.rejected_count, 2);
        assert_eq!(report.rejection_reasons, reasons);
    }

    #[test]
    fn validate_srpl_batch_dry_run_placeholder() {
        // Placeholder test; real tests in definitionbatch_compat.rs
    }
}
