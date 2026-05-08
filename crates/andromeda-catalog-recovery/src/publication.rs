use std::collections::BTreeSet;

use andromeda_catalog_store::{CatalogObjectRef, ObjectKind};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, DatabaseId, NamespaceId, ProcedureId};

/// Confines this contract surface to catalog Administration/HA publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationAudience {
    AdministrationHaOnly,
}

/// Categorical reason for a catalog publication trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationReasonCode {
    DefinitionBatchCommitted,
    RecoveryReplayRestored,
    HadrCatchupReplay,
}

/// Runtime class for administrative catalog subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSubscriberKind {
    Administration,
    HadrReplica,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationReplayTerminalOutcome {
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayRecordKind {
    Publication,
    SubscriptionAcknowledgement,
    Terminal,
}

/// Minimal audit fields required to correlate a catalog publication decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationAuditTrace {
    pub trace_id: String,
    pub operator_principal: String,
    pub reason_code: CatalogPublicationReasonCode,
}

impl CatalogPublicationAuditTrace {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.trim().is_empty() {
            return catalog_recovery_publication_error(
                "catalog publication audit trace id must not be empty",
            );
        }
        if self.operator_principal.trim().is_empty() {
            return catalog_recovery_publication_error(
                "catalog publication audit operator principal must not be empty",
            );
        }
        Ok(())
    }
}

/// Subscriber acknowledgement for a durable catalog publication boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker> {
    pub subscriber_id: TSubscriberId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub acknowledged_version: CatalogVersion,
    pub durable_lsn_seen: Option<u64>,
    pub durable_evidence_marker_seen: Option<TDurabilityMarker>,
    pub replayed_record_count: usize,
    pub audit_trace_id: String,
}

/// Publication durability envelope expected by acknowledgement checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReceiptExpectation<'a, TDurabilityMarker> {
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub next_version: CatalogVersion,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<&'a TDurabilityMarker>,
    pub record_count: usize,
    pub audit_trace_id: &'a str,
}

impl<TSubscriberId, TDurabilityMarker>
    CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker>
where
    TDurabilityMarker: PartialEq,
{
    pub fn validate_against_expectation(
        &self,
        expected: &CatalogPublicationReceiptExpectation<'_, TDurabilityMarker>,
    ) -> AndromedaResult<()> {
        if self.database_id != expected.database_id || self.namespace_id != expected.namespace_id {
            return catalog_recovery_publication_error(
                "catalog subscription acknowledgement identity must match publication",
            );
        }
        require_equal(
            &self.acknowledged_version,
            &expected.next_version,
            "catalog subscription acknowledgement version must match publication",
        )?;
        require_equal(
            &self.durable_lsn_seen,
            &expected.durable_lsn,
            "catalog subscription acknowledgement durable LSN must match publication",
        )?;
        require_equal(
            &self.durable_evidence_marker_seen.as_ref(),
            &expected.durable_evidence_marker,
            "catalog subscription acknowledgement durable marker must match publication",
        )?;
        require_equal(
            &self.replayed_record_count,
            &expected.record_count,
            "catalog subscription acknowledgement replayed record count must match publication",
        )?;
        require_equal(
            self.audit_trace_id.as_str(),
            expected.audit_trace_id,
            "catalog subscription acknowledgement audit trace id must match publication",
        )
    }
}

/// Evidence that the audit record was durably accepted before visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogVisibleChangeAuditEvidence<TDurabilityMarker> {
    pub trace: CatalogPublicationAuditTrace,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<TDurabilityMarker>,
    pub audit_record_ordinal: u64,
    pub visible_change_record_ordinal: u64,
}

impl<TDurabilityMarker> CatalogVisibleChangeAuditEvidence<TDurabilityMarker>
where
    TDurabilityMarker: PartialEq,
{
    pub fn validate_against_expectation(
        &self,
        expected_trace: &CatalogPublicationAuditTrace,
        expected_durable_lsn: Option<u64>,
        expected_durable_evidence_marker: Option<&TDurabilityMarker>,
    ) -> AndromedaResult<()> {
        self.trace.validate()?;
        require_equal(
            &self.trace,
            expected_trace,
            "catalog visible change audit evidence must match publication audit trace",
        )?;
        require_equal(
            &self.durable_lsn,
            &expected_durable_lsn,
            "catalog visible change audit durable LSN must match publication receipt",
        )?;
        require_equal(
            &self.durable_evidence_marker.as_ref(),
            &expected_durable_evidence_marker,
            "catalog visible change audit durable marker must match publication receipt",
        )?;
        if self.audit_record_ordinal == 0 || self.visible_change_record_ordinal == 0 {
            return catalog_recovery_publication_error(
                "catalog visible change audit ordinals must not be zero",
            );
        }
        if self.audit_record_ordinal >= self.visible_change_record_ordinal {
            return catalog_recovery_publication_error(
                "catalog visible change audit evidence must be recorded before the visible change",
            );
        }
        Ok(())
    }
}

/// Procedure contract identity participating in plan-cache invalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlanInvalidatedContract {
    pub procedure_id: ProcedureId,
    pub object: CatalogObjectRef,
    pub contract_hash: ContractHash,
}

impl CatalogPlanInvalidatedContract {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return catalog_recovery_publication_error(
                "published contract procedure id must not be zero",
            );
        }
        self.object.validate_for_definition(ObjectKind::Procedure)?;
        if self.object.catalog_version > visible_version {
            return catalog_recovery_publication_error(
                "published contract object version must not exceed the publication version",
            );
        }
        if self.contract_hash.is_zero() {
            return catalog_recovery_publication_error("published contract hash must not be zero");
        }
        Ok(())
    }
}

/// Bounded invalidation report for plan-cache subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlanInvalidationReport {
    pub catalog_version: CatalogVersion,
    pub changed_contracts: Vec<CatalogPlanInvalidatedContract>,
}

impl CatalogPlanInvalidationReport {
    pub fn validate_for_visible_version(
        &self,
        visible_version: CatalogVersion,
    ) -> AndromedaResult<()> {
        if self.catalog_version != visible_version {
            return catalog_recovery_publication_error(
                "plan invalidation catalog version must match publication next version",
            );
        }

        let mut procedure_ids = BTreeSet::new();
        let mut object_ids = BTreeSet::new();
        for contract in &self.changed_contracts {
            contract.validate_for_version(visible_version)?;
            if !procedure_ids.insert(contract.procedure_id) {
                return catalog_recovery_publication_error(
                    "plan invalidation report must not duplicate procedure ids",
                );
            }
            if !object_ids.insert(contract.object.object_id) {
                return catalog_recovery_publication_error(
                    "plan invalidation report must not duplicate procedure object ids",
                );
            }
        }

        Ok(())
    }
}

fn require_equal<T: PartialEq + ?Sized>(
    observed: &T,
    expected: &T,
    message: &'static str,
) -> AndromedaResult<()> {
    if observed == expected {
        Ok(())
    } else {
        catalog_recovery_publication_error(message)
    }
}

fn catalog_recovery_publication_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
}
