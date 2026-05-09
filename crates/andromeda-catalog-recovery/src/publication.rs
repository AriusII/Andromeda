use std::collections::{BTreeMap, BTreeSet};

use andromeda_catalog_store::{CatalogObjectRef, ObjectKind};
use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, DatabaseId, NamespaceId, ProcedureId};

use crate::CatalogPublicationSemantics;

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

pub trait CatalogPublicationReceiptView {
    type DurabilityMarker: Clone + PartialEq + Eq;

    fn batch_id_value(&self) -> u64;
    fn database_id(&self) -> DatabaseId;
    fn namespace_id(&self) -> NamespaceId;
    fn previous_version(&self) -> CatalogVersion;
    fn next_version(&self) -> CatalogVersion;
    fn durable_lsn(&self) -> Option<u64>;
    fn durable_evidence_marker(&self) -> Option<&Self::DurabilityMarker>;
    fn record_count(&self) -> usize;
    fn source_hash_is_zero(&self) -> bool;
    fn dependency_graph_hash_is_zero(&self) -> bool;
    fn publication_semantics(&self) -> CatalogPublicationSemantics;
}

pub trait CatalogPublicationBatchIdView {
    fn batch_id_value(&self) -> u64;
}

impl CatalogPublicationBatchIdView for DefinitionBatchId {
    fn batch_id_value(&self) -> u64 {
        self.get()
    }
}

pub trait CatalogPublicationHashEvidence {
    fn is_zero(&self) -> bool;
}

impl CatalogPublicationHashEvidence for DefinitionBatchSourceHash {
    fn is_zero(&self) -> bool {
        (*self).is_zero()
    }
}

impl CatalogPublicationHashEvidence for DefinitionBatchDependencyGraphHash {
    fn is_zero(&self) -> bool {
        (*self).is_zero()
    }
}

impl<BatchId, SourceHash, DependencyGraphHash> CatalogPublicationReceiptView
    for andromeda_catalog_store::CatalogPublicationReceipt<BatchId, SourceHash, DependencyGraphHash>
where
    BatchId: CatalogPublicationBatchIdView + Copy,
    SourceHash: CatalogPublicationHashEvidence + Copy,
    DependencyGraphHash: CatalogPublicationHashEvidence + Copy,
{
    type DurabilityMarker = andromeda_catalog_store::CatalogDurabilityMarker;

    fn batch_id_value(&self) -> u64 {
        CatalogPublicationBatchIdView::batch_id_value(&self.batch_id)
    }

    fn database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }

    fn durable_lsn(&self) -> Option<u64> {
        self.durable_lsn
    }

    fn durable_evidence_marker(&self) -> Option<&Self::DurabilityMarker> {
        self.durable_evidence_marker.as_ref()
    }

    fn record_count(&self) -> usize {
        self.record_count
    }

    fn source_hash_is_zero(&self) -> bool {
        self.source_hash.is_zero()
    }

    fn dependency_graph_hash_is_zero(&self) -> bool {
        self.dependency_graph_hash.is_zero()
    }

    fn publication_semantics(&self) -> CatalogPublicationSemantics {
        match self.publication_semantics {
            andromeda_catalog_store::CatalogPublicationSemantics::PlannedVersionOnly => {
                CatalogPublicationSemantics::PlannedVersionOnly
            },
            andromeda_catalog_store::CatalogPublicationSemantics::DurablePublicationExternal => {
                CatalogPublicationSemantics::DurablePublicationExternal
            },
        }
    }
}

pub trait CatalogSubscriberIdentity {
    fn catalog_subscriber_id(&self) -> &str;
}

impl CatalogSubscriberIdentity for String {
    fn catalog_subscriber_id(&self) -> &str {
        self.as_str()
    }
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

/// Object/version identity published for an applied catalog change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublishedObject {
    pub object: CatalogObjectRef,
}

impl CatalogPublishedObject {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)?;
        if self.object.catalog_version > visible_version {
            return catalog_recovery_publication_error(
                "published catalog object version must not exceed the publication version",
            );
        }
        Ok(())
    }
}

/// Back-compat alias for plan invalidation contract identities.
pub type CatalogPublishedContract = CatalogPlanInvalidatedContract;

/// Recovery/replay expectations attached to a durable publication.
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

impl<TReceipt> CatalogPublicationReport<TReceipt>
where
    TReceipt: CatalogPublicationReceiptView,
{
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.audience != CatalogPublicationAudience::AdministrationHaOnly {
            return catalog_recovery_publication_error(
                "catalog publication report audience must be Administration/HA only",
            );
        }
        validate_catalog_publication_receipt(&self.receipt)?;
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

/// Stable identity for publication/subscription recovery replay records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogPublicationReplayKey {
    pub batch_id: u64,
    pub database_id: u64,
    pub namespace_id: u64,
    pub previous_version: u64,
    pub next_version: u64,
}

impl CatalogPublicationReplayKey {
    pub fn from_receipt<TReceipt>(receipt: &TReceipt) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            batch_id: receipt.batch_id_value(),
            database_id: receipt.database_id().get(),
            namespace_id: receipt.namespace_id().get(),
            previous_version: receipt.previous_version().get(),
            next_version: receipt.next_version().get(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogSubscriptionReplayKey {
    pub subscriber_id: String,
    pub database_id: u64,
    pub namespace_id: u64,
    pub acknowledged_version: u64,
}

impl CatalogSubscriptionReplayKey {
    pub fn from_acknowledgement<TSubscriberId, TDurabilityMarker>(
        acknowledgement: &CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker>,
    ) -> Self
    where
        TSubscriberId: CatalogSubscriberIdentity,
    {
        Self {
            subscriber_id: acknowledgement
                .subscriber_id
                .catalog_subscriber_id()
                .to_owned(),
            database_id: acknowledgement.database_id.get(),
            namespace_id: acknowledgement.namespace_id.get(),
            acknowledged_version: acknowledgement.acknowledged_version.get(),
        }
    }
}

/// Durable terminal evidence observed while replaying a publication stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReplayTerminalRecord<TDurabilityMarker> {
    pub key: CatalogPublicationReplayKey,
    pub outcome: CatalogPublicationReplayTerminalOutcome,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<TDurabilityMarker>,
    pub record_count: usize,
    pub audit_trace_id: String,
}

impl<TDurabilityMarker> CatalogPublicationReplayTerminalRecord<TDurabilityMarker>
where
    TDurabilityMarker: Clone + PartialEq + Eq,
{
    pub fn committed_for_publication<TReceipt>(
        publication: &CatalogPublicationReport<TReceipt>,
    ) -> Self
    where
        TReceipt: CatalogPublicationReceiptView<DurabilityMarker = TDurabilityMarker>,
    {
        Self {
            key: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            outcome: CatalogPublicationReplayTerminalOutcome::Committed,
            durable_lsn: publication.receipt.durable_lsn(),
            durable_evidence_marker: publication.receipt.durable_evidence_marker().cloned(),
            record_count: publication.receipt.record_count(),
            audit_trace_id: publication.audit_trace.trace_id.clone(),
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.key.batch_id == 0
            || self.key.database_id == 0
            || self.key.namespace_id == 0
            || self.key.previous_version == 0
            || self.key.next_version <= self.key.previous_version
        {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal identity must describe a monotonic nonzero publication",
            );
        }
        let expected_next = self.key.previous_version.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication replay terminal version advance overflowed",
            )
        })?;
        if self.key.next_version != expected_next {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal must advance by exactly one catalog version",
            );
        }
        if self.record_count == 0 {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal record count must not be zero",
            );
        }
        if self.audit_trace_id.trim().is_empty() {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal audit trace id must not be empty",
            );
        }
        if self.durable_lsn == Some(0) {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal durable LSN must not be zero",
            );
        }
        if self.durable_lsn.is_none() && self.durable_evidence_marker.is_none() {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal must carry durable LSN or marker evidence",
            );
        }
        Ok(())
    }

    pub fn validate_for_publication<TReceipt>(
        &self,
        publication: &CatalogPublicationReport<TReceipt>,
    ) -> AndromedaResult<()>
    where
        TReceipt: CatalogPublicationReceiptView<DurabilityMarker = TDurabilityMarker>,
    {
        self.validate()?;
        if self.outcome != CatalogPublicationReplayTerminalOutcome::Committed {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal must be committed before applying visible publication",
            );
        }
        require_equal(
            &self.key,
            &CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            "catalog publication replay terminal identity must match publication",
        )?;
        require_equal(
            &self.durable_lsn,
            &publication.receipt.durable_lsn(),
            "catalog publication replay terminal durable LSN must match publication",
        )?;
        require_equal(
            &self.durable_evidence_marker.as_ref(),
            &publication.receipt.durable_evidence_marker(),
            "catalog publication replay terminal durable marker must match publication",
        )?;
        require_equal(
            &self.record_count,
            &publication.receipt.record_count(),
            "catalog publication replay terminal record count must match publication",
        )?;
        require_equal(
            &self.audit_trace_id,
            &publication.audit_trace.trace_id,
            "catalog publication replay terminal audit trace id must match publication",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayEvidence {
    PublicationApplied {
        key: CatalogPublicationReplayKey,
        audit_trace_id: String,
    },
    SubscriptionAcknowledged {
        key: CatalogSubscriptionReplayKey,
        publication: CatalogPublicationReplayKey,
    },
    TerminalObserved {
        key: CatalogPublicationReplayKey,
        outcome: CatalogPublicationReplayTerminalOutcome,
    },
    DuplicateIgnored {
        record_kind: CatalogPublicationSubscriptionReplayRecordKind,
    },
}

#[allow(
    clippy::large_enum_variant,
    reason = "Replay records intentionally carry complete catalog publication evidence for fail-closed comparison."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayRecord<TReceipt, TSubscriberId>
where
    TReceipt: CatalogPublicationReceiptView,
{
    VisiblePublication {
        publication: CatalogPublicationReport<TReceipt>,
        audit_evidence: CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>,
    },
    SubscriptionAcknowledgement(
        CatalogSubscriptionAcknowledgement<TSubscriberId, TReceipt::DurabilityMarker>,
    ),
    Terminal(CatalogPublicationReplayTerminalRecord<TReceipt::DurabilityMarker>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationSubscriptionReplaySummary {
    pub applied_publication_count: usize,
    pub acknowledged_subscription_count: usize,
    pub terminal_record_count: usize,
    pub duplicate_record_count: usize,
    pub audit_before_visible_change_count: usize,
    pub final_visible_catalog_version: Option<CatalogVersion>,
    pub evidence: Vec<CatalogPublicationSubscriptionReplayEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedPublication<TReceipt>
where
    TReceipt: CatalogPublicationReceiptView,
{
    publication: CatalogPublicationReport<TReceipt>,
    audit_evidence: CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PublishedVersionKey {
    database_id: u64,
    namespace_id: u64,
    catalog_version: u64,
}

impl PublishedVersionKey {
    fn from_publication<TReceipt>(publication: &CatalogPublicationReport<TReceipt>) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            database_id: publication.receipt.database_id().get(),
            namespace_id: publication.receipt.namespace_id().get(),
            catalog_version: publication.receipt.next_version().get(),
        }
    }

    fn from_acknowledgement<TSubscriberId, TDurabilityMarker>(
        acknowledgement: &CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker>,
    ) -> Self {
        Self {
            database_id: acknowledgement.database_id.get(),
            namespace_id: acknowledgement.namespace_id.get(),
            catalog_version: acknowledgement.acknowledged_version.get(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PublishedScopeKey {
    database_id: u64,
    namespace_id: u64,
}

impl PublishedScopeKey {
    fn from_publication<TReceipt>(publication: &CatalogPublicationReport<TReceipt>) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            database_id: publication.receipt.database_id().get(),
            namespace_id: publication.receipt.namespace_id().get(),
        }
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

pub fn catalog_visible_change_audit_evidence_for_publication<TReceipt>(
    publication: &CatalogPublicationReport<TReceipt>,
    audit_record_ordinal: u64,
    visible_change_record_ordinal: u64,
) -> CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>
where
    TReceipt: CatalogPublicationReceiptView,
{
    CatalogVisibleChangeAuditEvidence {
        trace: publication.audit_trace.clone(),
        durable_lsn: publication.receipt.durable_lsn(),
        durable_evidence_marker: publication.receipt.durable_evidence_marker().cloned(),
        audit_record_ordinal,
        visible_change_record_ordinal,
    }
}

pub fn validate_visible_change_audit_for_publication<TReceipt>(
    evidence: &CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>,
    publication: &CatalogPublicationReport<TReceipt>,
) -> AndromedaResult<()>
where
    TReceipt: CatalogPublicationReceiptView,
{
    publication.validate()?;
    evidence.validate_against_expectation(
        &publication.audit_trace,
        publication.receipt.durable_lsn(),
        publication.receipt.durable_evidence_marker(),
    )
}

pub fn validate_subscription_acknowledgement_for_publication<TReceipt, TSubscriberId>(
    acknowledgement: &CatalogSubscriptionAcknowledgement<TSubscriberId, TReceipt::DurabilityMarker>,
    publication: &CatalogPublicationReport<TReceipt>,
) -> AndromedaResult<()>
where
    TReceipt: CatalogPublicationReceiptView,
{
    publication.validate()?;
    acknowledgement.validate_against_expectation(&CatalogPublicationReceiptExpectation {
        database_id: publication.receipt.database_id(),
        namespace_id: publication.receipt.namespace_id(),
        next_version: publication.receipt.next_version(),
        durable_lsn: publication.receipt.durable_lsn(),
        durable_evidence_marker: publication.receipt.durable_evidence_marker(),
        record_count: publication.receipt.record_count(),
        audit_trace_id: publication.audit_trace.trace_id.as_str(),
    })
}

pub fn replay_publication_subscription_changes<TReceipt, TSubscriberId>(
    records: impl IntoIterator<
        Item = CatalogPublicationSubscriptionReplayRecord<TReceipt, TSubscriberId>,
    >,
) -> AndromedaResult<CatalogPublicationSubscriptionReplaySummary>
where
    TReceipt: CatalogPublicationReceiptView + Clone + PartialEq + Eq,
    TSubscriberId: CatalogSubscriberIdentity + Clone + PartialEq + Eq,
{
    let mut publications =
        BTreeMap::<CatalogPublicationReplayKey, AppliedPublication<TReceipt>>::new();
    let mut publications_by_version =
        BTreeMap::<PublishedVersionKey, CatalogPublicationReplayKey>::new();
    let mut visible_versions = BTreeMap::<PublishedScopeKey, u64>::new();
    let mut acknowledgements = BTreeMap::<
        CatalogSubscriptionReplayKey,
        CatalogSubscriptionAcknowledgement<TSubscriberId, TReceipt::DurabilityMarker>,
    >::new();
    let mut terminals = BTreeMap::<
        CatalogPublicationReplayKey,
        CatalogPublicationReplayTerminalRecord<TReceipt::DurabilityMarker>,
    >::new();
    let mut duplicate_record_count = 0usize;
    let mut audit_before_visible_change_count = 0usize;
    let mut evidence = Vec::new();

    for record in records {
        match record {
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication,
                audit_evidence,
            } => {
                publication.validate()?;
                validate_visible_change_audit_for_publication(&audit_evidence, &publication)?;
                let key = CatalogPublicationReplayKey::from_receipt(&publication.receipt);

                let Some(terminal) = terminals.get(&key) else {
                    return catalog_recovery_publication_error(
                        "catalog visible publication replay requires a committed durable terminal record",
                    );
                };
                terminal.validate_for_publication(&publication)?;

                let applied = AppliedPublication {
                    publication,
                    audit_evidence,
                };

                if let Some(existing) = publications.get(&key) {
                    if existing == &applied {
                        duplicate_record_count += 1;
                        evidence.push(
                            CatalogPublicationSubscriptionReplayEvidence::DuplicateIgnored {
                                record_kind:
                                    CatalogPublicationSubscriptionReplayRecordKind::Publication,
                            },
                        );
                        continue;
                    }
                    return catalog_recovery_publication_error(
                        "conflicting catalog publication replay record for the same publication identity",
                    );
                }

                let visible_key = PublishedVersionKey::from_publication(&applied.publication);
                if let Some(existing_key) = publications_by_version.get(&visible_key)
                    && existing_key != &key
                {
                    return catalog_recovery_publication_error(
                        "conflicting catalog publication replay record for the same visible catalog version",
                    );
                }

                let scope_key = PublishedScopeKey::from_publication(&applied.publication);
                if let Some(current_version) = visible_versions.get(&scope_key)
                    && applied.publication.receipt.previous_version().get() != *current_version
                {
                    return catalog_recovery_publication_error(
                        "catalog publication replay must preserve catalog version ordering",
                    );
                }

                publications_by_version.insert(visible_key, key);
                visible_versions
                    .insert(scope_key, applied.publication.receipt.next_version().get());
                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::PublicationApplied {
                        key,
                        audit_trace_id: applied.publication.audit_trace.trace_id.clone(),
                    },
                );
                publications.insert(key, applied);
                audit_before_visible_change_count += 1;
            },
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ) => {
                let version_key = PublishedVersionKey::from_acknowledgement(&acknowledgement);
                let Some(publication_key) = publications_by_version.get(&version_key).copied()
                else {
                    if let Some(publication) = publications.values().find(|publication| {
                        publication.publication.receipt.database_id() == acknowledgement.database_id
                            && publication.publication.receipt.namespace_id()
                                == acknowledgement.namespace_id
                    }) {
                        validate_subscription_acknowledgement_for_publication(
                            &acknowledgement,
                            &publication.publication,
                        )?;
                    }
                    return catalog_recovery_publication_error(
                        "catalog subscription acknowledgement replay requires a visible publication",
                    );
                };
                let Some(publication) = publications.get(&publication_key) else {
                    return catalog_recovery_publication_error(
                        "catalog subscription acknowledgement replay publication index is inconsistent",
                    );
                };
                validate_subscription_acknowledgement_for_publication(
                    &acknowledgement,
                    &publication.publication,
                )?;

                let acknowledgement_key =
                    CatalogSubscriptionReplayKey::from_acknowledgement(&acknowledgement);
                if let Some(existing) = acknowledgements.get(&acknowledgement_key) {
                    if existing == &acknowledgement {
                        duplicate_record_count += 1;
                        evidence.push(
                            CatalogPublicationSubscriptionReplayEvidence::DuplicateIgnored {
                                record_kind:
                                    CatalogPublicationSubscriptionReplayRecordKind::SubscriptionAcknowledgement,
                            },
                        );
                        continue;
                    }
                    return catalog_recovery_publication_error(
                        "conflicting catalog subscription acknowledgement replay record",
                    );
                }

                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::SubscriptionAcknowledged {
                        key: acknowledgement_key.clone(),
                        publication: publication_key,
                    },
                );
                acknowledgements.insert(acknowledgement_key, acknowledgement);
            },
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal) => {
                terminal.validate()?;
                if let Some(existing) = terminals.get(&terminal.key) {
                    if existing == &terminal {
                        duplicate_record_count += 1;
                        evidence.push(
                            CatalogPublicationSubscriptionReplayEvidence::DuplicateIgnored {
                                record_kind:
                                    CatalogPublicationSubscriptionReplayRecordKind::Terminal,
                            },
                        );
                        continue;
                    }
                    return catalog_recovery_publication_error(
                        "conflicting catalog publication replay terminal record",
                    );
                }

                if let Some(applied) = publications.get(&terminal.key) {
                    terminal.validate_for_publication(&applied.publication)?;
                }

                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::TerminalObserved {
                        key: terminal.key,
                        outcome: terminal.outcome,
                    },
                );
                terminals.insert(terminal.key, terminal);
            },
        }
    }

    let final_visible_catalog_version = publications
        .values()
        .map(|publication| publication.publication.receipt.next_version())
        .max_by_key(|version| version.get());

    Ok(CatalogPublicationSubscriptionReplaySummary {
        applied_publication_count: publications.len(),
        acknowledged_subscription_count: acknowledgements.len(),
        terminal_record_count: terminals.len(),
        duplicate_record_count,
        audit_before_visible_change_count,
        final_visible_catalog_version,
        evidence,
    })
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
