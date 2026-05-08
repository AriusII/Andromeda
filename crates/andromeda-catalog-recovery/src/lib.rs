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

mod mutation;
mod publication;

pub use mutation::{
    CatalogDurableMutationPayload, CatalogRecoveryAnomalyKind, CatalogSkippedBatchReason,
};
pub use publication::{
    CatalogPublicationAudience, CatalogPublicationReasonCode,
    CatalogPublicationReplayTerminalOutcome, CatalogPublicationSubscriptionReplayRecordKind,
    CatalogSubscriberKind,
};
