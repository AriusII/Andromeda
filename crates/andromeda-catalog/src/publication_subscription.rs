//! Typed catalog publication/subscription contract surface.
//!
//! This module is **Administration/HA only**.  It does not expose application
//! traffic, SQL, gRPC, JSON, storage I/O, or a subscriber runtime.  The types
//! here document and validate the bounded evidence that must travel between a
//! durable catalog publication producer and administrative/HA subscribers.

use std::collections::BTreeSet;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, DatabaseId,
    NamespaceId, ProcedureId,
};

use crate::{
    CatalogDurabilityMarker, CatalogObjectRef, CatalogPublicationReceipt,
    CatalogPublicationSemantics, ObjectKind,
};

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
            return catalog_publication_error(
                "catalog publication audit trace id must not be empty",
            );
        }
        if self.operator_principal.trim().is_empty() {
            return catalog_publication_error(
                "catalog publication audit operator principal must not be empty",
            );
        }
        Ok(())
    }
}

/// Object/version identity published for an applied catalog change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublishedObject {
    pub object: CatalogObjectRef,
}

impl CatalogPublishedObject {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)?;
        if self.object.catalog_version > visible_version {
            return catalog_publication_error(
                "published catalog object version must not exceed the publication version",
            );
        }
        Ok(())
    }
}

/// Procedure contract identity that participates in plan-cache invalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublishedContract {
    pub procedure_id: ProcedureId,
    pub object: CatalogObjectRef,
    pub contract_hash: ContractHash,
}

impl CatalogPublishedContract {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return catalog_publication_error("published contract procedure id must not be zero");
        }
        self.object.validate_for_definition(ObjectKind::Procedure)?;
        if self.object.catalog_version > visible_version {
            return catalog_publication_error(
                "published contract object version must not exceed the publication version",
            );
        }
        if self.contract_hash.is_zero() {
            return catalog_publication_error("published contract hash must not be zero");
        }
        Ok(())
    }
}

/// Bounded invalidation report for future plan-cache subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlanInvalidationReport {
    pub catalog_version: CatalogVersion,
    pub changed_contracts: Vec<CatalogPublishedContract>,
}

impl CatalogPlanInvalidationReport {
    pub fn validate_for_receipt(&self, receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
        if self.catalog_version != receipt.next_version {
            return catalog_publication_error(
                "plan invalidation catalog version must match publication next version",
            );
        }

        let mut procedure_ids = BTreeSet::new();
        let mut object_ids = BTreeSet::new();
        for contract in &self.changed_contracts {
            contract.validate_for_version(receipt.next_version)?;
            if !procedure_ids.insert(contract.procedure_id) {
                return catalog_publication_error(
                    "plan invalidation report must not duplicate procedure ids",
                );
            }
            if !object_ids.insert(contract.object.object_id) {
                return catalog_publication_error(
                    "plan invalidation report must not duplicate procedure object ids",
                );
            }
        }

        Ok(())
    }
}

/// Recovery/replay expectations attached to a durable publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRecoveryReplayExpectation {
    pub starting_version: CatalogVersion,
    pub target_version: CatalogVersion,
    pub required_record_count: usize,
    pub require_exact_commit_boundary: bool,
}

impl CatalogRecoveryReplayExpectation {
    pub const fn from_receipt(receipt: &CatalogPublicationReceipt) -> Self {
        Self {
            starting_version: receipt.previous_version,
            target_version: receipt.next_version,
            required_record_count: receipt.record_count,
            require_exact_commit_boundary: true,
        }
    }

    pub fn validate_for_receipt(&self, receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
        if self.starting_version != receipt.previous_version
            || self.target_version != receipt.next_version
        {
            return catalog_publication_error(
                "recovery replay expectation versions must match publication receipt",
            );
        }
        if self.required_record_count != receipt.record_count {
            return catalog_publication_error(
                "recovery replay expectation record count must match publication receipt",
            );
        }
        if !self.require_exact_commit_boundary {
            return catalog_publication_error(
                "catalog recovery replay must require exact commit boundary matching",
            );
        }
        Ok(())
    }
}

/// Durable publication report consumed only by administrative/HA subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReport {
    pub audience: CatalogPublicationAudience,
    pub receipt: CatalogPublicationReceipt,
    pub published_objects: Vec<CatalogPublishedObject>,
    pub plan_invalidation: CatalogPlanInvalidationReport,
    pub recovery_replay: CatalogRecoveryReplayExpectation,
    pub audit_trace: CatalogPublicationAuditTrace,
}

impl CatalogPublicationReport {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.audience != CatalogPublicationAudience::AdministrationHaOnly {
            return catalog_publication_error(
                "catalog publication report audience must be Administration/HA only",
            );
        }
        validate_receipt(&self.receipt)?;
        self.plan_invalidation.validate_for_receipt(&self.receipt)?;
        self.recovery_replay.validate_for_receipt(&self.receipt)?;
        self.audit_trace.validate()?;

        let mut object_ids = BTreeSet::new();
        for published in &self.published_objects {
            published.validate_for_version(self.receipt.next_version)?;
            if !object_ids.insert(published.object.object_id) {
                return catalog_publication_error(
                    "catalog publication report must not duplicate published object ids",
                );
            }
        }

        Ok(())
    }
}

/// Stable identifier for an administrative/HA catalog subscriber.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogSubscriberId(String);

impl CatalogSubscriberId {
    pub fn new(value: impl Into<String>) -> AndromedaResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return catalog_publication_error("catalog subscriber id must not be empty");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A subscriber acknowledgement for a durable catalog publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriptionAcknowledgement {
    pub subscriber_id: CatalogSubscriberId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub acknowledged_version: CatalogVersion,
    pub durable_lsn_seen: Option<u64>,
    pub durable_evidence_marker_seen: Option<CatalogDurabilityMarker>,
    pub replayed_record_count: usize,
    pub audit_trace_id: String,
}

impl CatalogSubscriptionAcknowledgement {
    pub fn validate_for_publication(
        &self,
        publication: &CatalogPublicationReport,
    ) -> AndromedaResult<()> {
        publication.validate()?;
        if self.database_id != publication.receipt.database_id
            || self.namespace_id != publication.receipt.namespace_id
        {
            return catalog_publication_error(
                "catalog subscription acknowledgement identity must match publication",
            );
        }
        if self.acknowledged_version != publication.receipt.next_version {
            return catalog_publication_error(
                "catalog subscription acknowledgement version must match publication",
            );
        }
        if self.durable_lsn_seen != publication.receipt.durable_lsn {
            return catalog_publication_error(
                "catalog subscription acknowledgement durable LSN must match publication",
            );
        }
        if self.durable_evidence_marker_seen != publication.receipt.durable_evidence_marker {
            return catalog_publication_error(
                "catalog subscription acknowledgement durable marker must match publication",
            );
        }
        if self.replayed_record_count != publication.receipt.record_count {
            return catalog_publication_error(
                "catalog subscription acknowledgement replayed record count must match publication",
            );
        }
        if self.audit_trace_id != publication.audit_trace.trace_id {
            return catalog_publication_error(
                "catalog subscription acknowledgement audit trace id must match publication",
            );
        }
        Ok(())
    }
}

fn validate_receipt(receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
    if receipt.database_id.get() == 0 || receipt.namespace_id.get() == 0 {
        return catalog_publication_error(
            "catalog publication receipt identity fields must not be zero",
        );
    }
    if receipt.previous_version.get() == 0 || receipt.next_version <= receipt.previous_version {
        return catalog_publication_error(
            "catalog publication receipt must advance a nonzero catalog version",
        );
    }
    if receipt.record_count == 0 {
        return catalog_publication_error(
            "catalog publication receipt record count must not be zero",
        );
    }
    if receipt.publication_semantics != CatalogPublicationSemantics::DurablePublicationExternal {
        return catalog_publication_error(
            "catalog publication report requires durable publication semantics",
        );
    }
    if receipt.durable_lsn.is_none() && receipt.durable_evidence_marker.is_none() {
        return catalog_publication_error(
            "catalog publication receipt must carry durable WAL LSN or durable marker evidence",
        );
    }
    Ok(())
}

fn catalog_publication_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
}
