//! SRPL Procedure manifest validation during DefinitionBatch dry-run phase.
//!
//! This module validates Procedure contracts that have already been materialized
//! by the SRPL compiler. `andromeda-catalog` intentionally does not parse SRPL
//! source: `andromeda-srpl` depends on `andromeda-catalog`, so compiling source
//! here would create a crate dependency cycle. Source-level parse, bind, lower,
//! and manifest materialization must happen in `andromeda-srpl` before a
//! `CatalogDefinition::Procedure` enters a DefinitionBatch.
//!
//! ## Validation Pipeline
//!
//! For each Procedure contract in the batch:
//! 1. **Contract hash**: Verify the stored `ContractHash` matches the canonical contract shape.
//! 2. **Binding**: Verify `ProcedureId`, `CatalogVersion`, `ContractHash`, `StatsVersion`, and `PolicyVersion`.
//! 3. **Manifest readiness**: Count valid Procedure manifests before object mutation planning.
//!
//! If all procedures validate, dry-run returns success with affected catalog version.
//! If any procedure fails, dry-run returns failure with diagnostic (entire batch rejected).
//!
//! ## Error Categories
//!
//! Source diagnostics are emitted by `andromeda-srpl`. Catalog diagnostics here
//! are contract or manifest integrity diagnostics.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{CatalogDefinition, DefinitionBatch, DefinitionOperation, ObjectKind};

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

/// Validate all materialized Procedure manifests in a batch during dry-run phase.
///
/// This function is called by DefinitionBatch::dry_run() before applying mutations.
/// It does not compile SRPL source; it validates that each Procedure contract
/// already carries canonical manifest and binding evidence.
///
/// # Arguments
///
/// * `batch` - The batch containing materialized Procedure definitions to validate
///
/// # Returns
///
/// On success: SrplBatchDryRunReport with all_valid=true
/// On failure: AndromedaError (entire batch rejected due to failed Procedure manifests)
///
/// # Atomicity
///
/// This function implements all-or-nothing semantics:
/// - If any procedure fails validation, the entire batch is rejected
/// - Partial success is not allowed
/// - Error diagnostic includes all failed procedures
pub fn validate_srpl_batch_dry_run(
    batch: &DefinitionBatch,
) -> AndromedaResult<SrplBatchDryRunReport> {
    let mut valid_count = 0;
    let mut rejection_reasons = Vec::new();

    for (operation_index, operation) in batch.operations.iter().enumerate() {
        let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) = operation else {
            continue;
        };

        let procedure_name = contract.object.name.as_catalog_path();
        if contract.object.kind != ObjectKind::Procedure {
            rejection_reasons.push(format!(
                "operation {operation_index} procedure {procedure_name}: catalog object kind must be Procedure"
            ));
            continue;
        }

        if let Err(error) = contract.validate_canonical_hash() {
            rejection_reasons.push(format!(
                "operation {operation_index} procedure {procedure_name}: {}",
                error.message()
            ));
            continue;
        }

        if let Err(error) = contract.binding().validate() {
            rejection_reasons.push(format!(
                "operation {operation_index} procedure {procedure_name}: {}",
                error.message()
            ));
            continue;
        }

        valid_count += 1;
    }

    if rejection_reasons.is_empty() {
        return Ok(SrplBatchDryRunReport::success(valid_count));
    }

    let report = SrplBatchDryRunReport::failure(valid_count, rejection_reasons);
    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "definition batch SRPL dry-run rejected {} procedure(s): {}",
            report.rejected_count,
            report.rejection_reasons.join("; ")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DefinitionBatchId, INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID,
        inventory_domain_definition_batch, inventory_reserve_stock_contract,
    };
    use andromeda_core::{AndromedaErrorKind, CatalogVersion};

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
    fn srpl_batch_dry_run_counts_materialized_procedure_contracts() {
        let batch = inventory_domain_definition_batch().unwrap();
        let report = validate_srpl_batch_dry_run(&batch).unwrap();

        assert!(report.all_valid);
        assert_eq!(report.valid_count, 1);
        assert_eq!(report.rejected_count, 0);
    }

    #[test]
    fn srpl_batch_dry_run_rejects_stale_procedure_manifest_hash() {
        let mut contract = inventory_reserve_stock_contract().unwrap();
        contract.stats_version = crate::StatsVersion::new(contract.stats_version.get() + 1);
        let batch = DefinitionBatch {
            batch_id: DefinitionBatchId::new(0xD7),
            database_id: INVENTORY_DATABASE_ID,
            namespace_id: INVENTORY_NAMESPACE_ID,
            base_version: CatalogVersion::new(0),
            operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                contract,
            ))],
        };

        let error = batch.dry_run().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("SRPL dry-run rejected"));
        assert!(error.message().contains("canonical contract shape"));
    }
}
