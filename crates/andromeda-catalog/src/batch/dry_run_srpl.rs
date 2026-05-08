//! Compatibility facade for materialized Procedure validation during
//! DefinitionBatch dry-run.
//!
//! Catalog does not parse SRPL source. The portable contract validation lives
//! in `andromeda-definition-batch`; source-level parse, bind, lower, and
//! manifest materialization remain owned by `andromeda-srpl`.

pub use andromeda_definition_batch::SrplBatchDryRunReport;
use andromeda_error::AndromedaResult;

use crate::DefinitionBatch;

pub fn validate_srpl_batch_dry_run(
    batch: &DefinitionBatch,
) -> AndromedaResult<SrplBatchDryRunReport> {
    andromeda_definition_batch::validate_srpl_operations_dry_run(&batch.operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CatalogDefinition, DefinitionBatchId, DefinitionOperation, INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID, inventory_domain_definition_batch,
        inventory_reserve_stock_contract,
    };
    use andromeda_error::AndromedaErrorKind;
    use andromeda_types::CatalogVersion;

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
