use andromeda_error::AndromedaResult;

use crate::CatalogPublicationReceipt;

pub type CatalogPlanInvalidatedContract =
    andromeda_catalog_recovery::CatalogPlanInvalidatedContract;
pub type CatalogPlanInvalidationReport = andromeda_catalog_recovery::CatalogPlanInvalidationReport;

pub fn validate_plan_invalidation_for_receipt(
    report: &CatalogPlanInvalidationReport,
    receipt: &CatalogPublicationReceipt,
) -> AndromedaResult<()> {
    report.validate_for_visible_version(receipt.next_version)
}
