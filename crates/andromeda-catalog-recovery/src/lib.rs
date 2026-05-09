#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Recovery Contracts

`andromeda-catalog-recovery` owns dependency-light contract types shared by
catalog recovery and catalog publication replay.

The catalog crate remains the owner for APIs that need live catalog state,
including `CatalogSnapshot` storage and compatibility facades returning
snapshot-owned outcomes. This crate owns the runtime-free replay and
publication/subscriber state machines behind traits and generic DTOs.

## Owner Boundary

| Contract group | Owner |
| --- | --- |
| Durable payload envelope and replay classifications | `andromeda-catalog-recovery` |
| Publication audience and replay classifications | `andromeda-catalog-recovery` |
| Generic recovery replay orchestration | `andromeda-catalog-recovery` |
| WAL payload decoding | `andromeda-catalog-recovery` |
| Publication report, receipt, and subscriber registry validation | `andromeda-catalog-recovery` |
| Snapshot-owned recovery facades | `andromeda-catalog` |

"#]

mod durable_payload;
mod lsn_replay;
mod mutation;
mod mutation_replay;
mod publication;
mod publication_replay;
mod replay_report;
mod storage_wal_bridge;
mod storage_wal_record;
mod wal_record;

pub use durable_payload::{
    decode_catalog_durable_payload, encode_catalog_durable_payload,
    validate_catalog_mutation_record,
};
pub use lsn_replay::{
    CatalogLsnReplaySelection, CatalogReplayFromLsnReport, LsnBoundCatalogRecord,
};
pub use mutation::{
    CATALOG_CHANGE_APPLY_WAL_KIND_TAG, CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
    CATALOG_CHANGE_COMMIT_WAL_KIND_TAG, CatalogDurableMutationPayload, CatalogLifecycleTarget,
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogMutationRecordKind, CatalogPublicationSemantics, CatalogRecoveryAnomalyKind,
    CatalogRecoveryApplyTarget, CatalogSkippedBatchReason, CatalogWalPayloadDecodeError,
    CatalogWalPayloadDecodeErrorKind, DefinitionBatchDependencyGraphHash,
};
pub use mutation_replay::{
    CatalogRecoveryTargetOutcome, IndexedCatalogMutationRecord,
    replay_catalog_mutation_records_into_target,
    replay_indexed_catalog_mutation_records_into_target,
};
pub use publication::{
    CatalogHadrSubscriberReplayEvidence, CatalogPlanInvalidatedContract,
    CatalogPlanInvalidationReport, CatalogPublicationAudience, CatalogPublicationAuditTrace,
    CatalogPublicationBatchIdView, CatalogPublicationHashEvidence, CatalogPublicationReasonCode,
    CatalogPublicationReceiptExpectation, CatalogPublicationReceiptView,
    CatalogPublicationReplayKey, CatalogPublicationReplayTerminalOutcome,
    CatalogPublicationReplayTerminalRecord, CatalogPublicationReport,
    CatalogPublicationRuntimeState, CatalogPublicationSubscriberRegistry,
    CatalogPublicationSubscriptionReplayEvidence, CatalogPublicationSubscriptionReplayRecord,
    CatalogPublicationSubscriptionReplayRecordKind, CatalogPublicationSubscriptionReplaySummary,
    CatalogPublishedContract, CatalogPublishedObject, CatalogRecoveryReplayExpectation,
    CatalogSubscriberAckProgress, CatalogSubscriberId, CatalogSubscriberIdentity,
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
pub use storage_wal_bridge::{
    CatalogStorageWalDurablePublication, CatalogStorageWalPublicationRecord,
    CatalogStorageWalPublicationReplayReport, decode_storage_catalog_record,
    encode_storage_catalog_record, replay_storage_catalog_publications_from_wal,
};
pub use storage_wal_record::{CatalogStorageWalRecord, CatalogStorageWalRecordVersion};
pub use wal_record::{
    AlterCompatibilityPolicy, CatalogWalRecord, CatalogWalRecordDesign, DropFailureReason,
};
