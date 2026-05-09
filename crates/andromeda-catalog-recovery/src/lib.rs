#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Recovery Contracts

`andromeda-catalog-recovery` owns dependency-light contract types shared by
catalog recovery and catalog publication replay.

The catalog crate remains the owner for APIs that need live catalog state,
including snapshot replay, durable payload decoding, publication receipt
validation, and subscriber registry mutation. Keeping those APIs in
`andromeda-catalog` avoids a circular dependency while making the portable
classification and envelope contracts compile-safe here.

## Owner Boundary

| Contract group | Owner |
| --- | --- |
| Durable payload envelope and replay classifications | `andromeda-catalog-recovery` |
| Publication audience and replay classifications | `andromeda-catalog-recovery` |
| Snapshot recovery orchestration | `andromeda-catalog` |
| WAL payload decoding | `andromeda-catalog` |
| Publication report, receipt, and subscriber registry validation | `andromeda-catalog` |

"#]

mod durable_payload;
mod mutation;
mod publication;
mod publication_replay;
mod replay_report;
mod wal_record;

pub use durable_payload::{
    decode_catalog_durable_payload, encode_catalog_durable_payload,
    validate_catalog_mutation_record,
};
pub use mutation::{
    CATALOG_CHANGE_APPLY_WAL_KIND_TAG, CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
    CATALOG_CHANGE_COMMIT_WAL_KIND_TAG, CatalogDurableMutationPayload, CatalogLifecycleTarget,
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogMutationRecordKind, CatalogPublicationSemantics, CatalogRecoveryAnomalyKind,
    CatalogRecoveryApplyTarget, CatalogSkippedBatchReason, CatalogWalPayloadDecodeError,
    CatalogWalPayloadDecodeErrorKind, DefinitionBatchDependencyGraphHash,
};
pub use publication::{
    CatalogPlanInvalidatedContract, CatalogPlanInvalidationReport, CatalogPublicationAudience,
    CatalogPublicationAuditTrace, CatalogPublicationBatchIdView, CatalogPublicationHashEvidence,
    CatalogPublicationReasonCode, CatalogPublicationReceiptExpectation,
    CatalogPublicationReceiptView, CatalogPublicationReplayKey,
    CatalogPublicationReplayTerminalOutcome, CatalogPublicationReplayTerminalRecord,
    CatalogPublicationReport, CatalogPublicationSubscriptionReplayEvidence,
    CatalogPublicationSubscriptionReplayRecord, CatalogPublicationSubscriptionReplayRecordKind,
    CatalogPublicationSubscriptionReplaySummary, CatalogPublishedContract, CatalogPublishedObject,
    CatalogRecoveryReplayExpectation, CatalogSubscriberId, CatalogSubscriberIdentity,
    CatalogSubscriberKind, CatalogSubscriberRegistration, CatalogSubscriptionAcknowledgement,
    CatalogSubscriptionReplayKey, CatalogVisibleChangeAuditEvidence,
    catalog_visible_change_audit_evidence_for_publication, replay_publication_subscription_changes,
    validate_catalog_publication_receipt, validate_subscription_acknowledgement_for_publication,
    validate_visible_change_audit_for_publication,
};
pub use publication_replay::{
    CatalogWalDurablePublication, CatalogWalObservedRecord, CatalogWalPublicationRecord,
    CatalogWalPublicationRecordMetadata, CatalogWalPublicationReplayReport,
    replay_catalog_publications_from_observed_wal,
};
pub use replay_report::{
    CatalogRecoveredBatch, CatalogRecoveryAnomaly, CatalogRecoveryReport, CatalogSkippedBatch,
    recovery_anomaly_kind_for_decode_error,
};
pub use wal_record::{
    AlterCompatibilityPolicy, CatalogWalRecord, CatalogWalRecordDesign, DropFailureReason,
};
