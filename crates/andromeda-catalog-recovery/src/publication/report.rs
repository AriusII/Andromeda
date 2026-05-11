use std::collections::BTreeSet;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::CatalogVersion;

use crate::CatalogPublicationSemantics;

use super::{
    CatalogPlanInvalidationReport, CatalogPublicationAudience, CatalogPublicationAuditTrace,
    CatalogPublicationReceiptView, CatalogPublishedObject, catalog_recovery_publication_error,
    require_equal,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRecoveryReplayExpectation {
    pub starting_version: CatalogVersion,
    pub target_version: CatalogVersion,
    pub required_record_count: usize,
    pub require_exact_commit_boundary: bool,
}

impl CatalogRecoveryReplayExpectation {
    pub fn from_receipt<TReceipt>(receipt: &TReceipt) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            starting_version: receipt.previous_version(),
            target_version: receipt.next_version(),
            required_record_count: receipt.record_count(),
            require_exact_commit_boundary: true,
        }
    }

    pub fn validate_for_receipt<TReceipt>(&self, receipt: &TReceipt) -> AndromedaResult<()>
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        require_equal(
            &self.starting_version,
            &receipt.previous_version(),
            "recovery replay expectation starting version must match publication receipt",
        )?;
        require_equal(
            &self.target_version,
            &receipt.next_version(),
            "recovery replay expectation target version must match publication receipt",
        )?;
        require_equal(
            &self.required_record_count,
            &receipt.record_count(),
            "recovery replay expectation record count must match publication receipt",
        )?;
        let expected_next = self.starting_version.get().checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "recovery replay expectation version advance overflowed",
            )
        })?;
        if self.target_version.get() != expected_next {
            return catalog_recovery_publication_error(
                "recovery replay expectation must advance by exactly one catalog version",
            );
        }
        if !self.require_exact_commit_boundary {
            return catalog_recovery_publication_error(
                "catalog recovery replay must require exact commit boundary matching",
            );
        }
        Ok(())
    }
}

/// Durable publication report consumed only by administrative/HA subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReport<TReceipt> {
    pub audience: CatalogPublicationAudience,
    pub receipt: TReceipt,
    pub published_objects: Vec<CatalogPublishedObject>,
    pub plan_invalidation: CatalogPlanInvalidationReport,
    pub recovery_replay: CatalogRecoveryReplayExpectation,
    pub audit_trace: CatalogPublicationAuditTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationDurableEvidence<TDurabilityMarker> {
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<TDurabilityMarker>,
}

impl<TDurabilityMarker> CatalogPublicationDurableEvidence<TDurabilityMarker>
where
    TDurabilityMarker: PartialEq,
{
    pub fn validate_against_receipt<TReceipt>(&self, receipt: &TReceipt) -> AndromedaResult<()>
    where
        TReceipt: CatalogPublicationReceiptView<DurabilityMarker = TDurabilityMarker>,
    {
        if self.durable_lsn == Some(0) {
            return catalog_recovery_publication_error(
                "catalog publication receipt durable WAL LSN must not be zero",
            );
        }
        if self.durable_lsn.is_none() && self.durable_evidence_marker.is_none() {
            return catalog_recovery_publication_error(
                "catalog publication receipt must carry durable WAL LSN or durable marker evidence",
            );
        }
        require_equal(
            &self.durable_lsn,
            &receipt.durable_lsn(),
            "catalog publication durable evidence LSN must match publication receipt",
        )?;
        require_equal(
            &self.durable_evidence_marker.as_ref(),
            &receipt.durable_evidence_marker(),
            "catalog publication durable evidence marker must match publication receipt",
        )?;
        Ok(())
    }
}

impl<TReceipt> CatalogPublicationReport<TReceipt>
where
    TReceipt: CatalogPublicationReceiptView,
{
    pub fn durable_evidence(
        &self,
    ) -> CatalogPublicationDurableEvidence<TReceipt::DurabilityMarker> {
        CatalogPublicationDurableEvidence {
            durable_lsn: self.receipt.durable_lsn(),
            durable_evidence_marker: self.receipt.durable_evidence_marker().cloned(),
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.audience != CatalogPublicationAudience::AdministrationHaOnly {
            return catalog_recovery_publication_error(
                "catalog publication report audience must be Administration/HA only",
            );
        }
        validate_catalog_publication_receipt(&self.receipt)?;
        self.durable_evidence()
            .validate_against_receipt(&self.receipt)?;
        self.plan_invalidation
            .validate_for_visible_version(self.receipt.next_version())?;
        self.recovery_replay.validate_for_receipt(&self.receipt)?;
        self.audit_trace.validate()?;

        let mut object_ids = BTreeSet::new();
        for published in &self.published_objects {
            published.validate_for_version(self.receipt.next_version())?;
            if !object_ids.insert(published.object.object_id) {
                return catalog_recovery_publication_error(
                    "catalog publication report must not duplicate published object ids",
                );
            }
        }

        Ok(())
    }
}

pub fn validate_catalog_publication_receipt<TReceipt>(receipt: &TReceipt) -> AndromedaResult<()>
where
    TReceipt: CatalogPublicationReceiptView,
{
    if receipt.batch_id_value() == 0
        || receipt.database_id().get() == 0
        || receipt.namespace_id().get() == 0
    {
        return catalog_recovery_publication_error(
            "catalog publication receipt identity fields must not be zero",
        );
    }
    if receipt.previous_version().get() == 0 || receipt.next_version() <= receipt.previous_version()
    {
        return catalog_recovery_publication_error(
            "catalog publication receipt must advance a nonzero catalog version",
        );
    }
    let expected_next = receipt
        .previous_version()
        .get()
        .checked_add(1)
        .ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication receipt version advance overflowed",
            )
        })?;
    if receipt.next_version().get() != expected_next {
        return catalog_recovery_publication_error(
            "catalog publication receipt must advance by exactly one catalog version",
        );
    }
    if receipt.record_count() == 0 {
        return catalog_recovery_publication_error(
            "catalog publication receipt record count must not be zero",
        );
    }
    if receipt.source_hash_is_zero() || receipt.dependency_graph_hash_is_zero() {
        return catalog_recovery_publication_error(
            "catalog publication receipt DefinitionBatch hashes must not be zero",
        );
    }
    if receipt.publication_semantics() != CatalogPublicationSemantics::DurablePublicationExternal {
        return catalog_recovery_publication_error(
            "catalog publication report requires durable publication semantics",
        );
    }
    if receipt.durable_lsn() == Some(0) {
        return catalog_recovery_publication_error(
            "catalog publication receipt durable WAL LSN must not be zero",
        );
    }
    if receipt.durable_lsn().is_none() && receipt.durable_evidence_marker().is_none() {
        return catalog_recovery_publication_error(
            "catalog publication receipt must carry durable WAL LSN or durable marker evidence",
        );
    }
    Ok(())
}
