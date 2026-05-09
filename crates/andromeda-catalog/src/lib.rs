#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Catalog Engine

The canonical system catalog, object version management, and contract tracking.

## Overview

The catalog module owns:
- **Object Definitions**: Tables, procedures, structured objects, enums
- **Object Versioning**: Catalog versions and compatibility tracking
- **Contract Hashes**: Stable hash computation for procedure contracts
- **Definition Batches**: Atomic multi-object updates to the catalog

## Core Concepts

### Catalog Objects

Objects in the catalog include:
- **Tables**: Named relations with columns
- **Procedures**: User-defined operations with input/output contracts
- **Structured Objects**: Named tuple types
- **Enums**: Named scalar types with discrete values

Each object has a unique `CatalogObjectId` and a version number that increments
when the object definition changes.

### Object References and Bindings

`CatalogObjectRef` provides a snapshot view of an object at a specific catalog version.
`CatalogObjectBinding` represents dependencies (e.g., a procedure that reads a table).

### Contracts

Procedure contracts define:
- Input and output column schemas
- Protocol layout (protobuf descriptor set)
- Transaction policies (isolation, access mode)
- Compatibility policies (AdditiveOnly, ExactHash)
- Error handling policies
- Result stream metadata

Contract hashes provide stable, deterministic fingerprints of procedure behavior,
enabling:
- Protocol version compatibility checking
- Contract evolution tracking
- Change impact analysis

### Definition Batches

Batch operations allow atomic application of multiple object definitions,
maintaining catalog consistency across related changes.

## Module Organization

| Module | Purpose |
|--------|---------|
| `andromeda-catalog-store` | Catalog object definitions and qualified names |
| `andromeda-procedure-contract` | Procedure contracts and compatibility checking |
| `batch` | Transactional batch operations |
| `store` | Catalog storage interface |
| `snapshot` | Point-in-time catalog snapshots |

## Safety

This crate forbids unsafe code (`#![forbid(unsafe_code)]`).

"#]

mod batch;
pub mod digest;
mod recovery;
mod server;
mod snapshot;
mod store;
mod wal_integration;
mod wal_record;

pub use andromeda_catalog_store::{
    CatalogBindingKind, CatalogDefinition, CatalogObjectBinding, CatalogObjectRef, EnumDefinition,
    EnumVariant, ObjectKind, QualifiedName, StructuredObjectDefinition, TableDefinition,
};
pub use batch::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogDefinitionBatchPlanning,
    CatalogDurabilityMarker, CatalogLifecycleAction, CatalogLifecycleTarget, CatalogMutation,
    CatalogMutationBoundary, CatalogMutationCommitEvidence, CatalogMutationDelta,
    CatalogMutationDurability, CatalogMutationOperation, CatalogMutationPlan,
    CatalogMutationRecord, CatalogMutationRecordKind, CatalogPublicationReceipt,
    CatalogPublicationSemantics, CatalogWalPayloadDecodeError, CatalogWalPayloadDecodeErrorKind,
    DefinitionBatch, DefinitionBatchId, DefinitionBatchImportId, DefinitionBatchPlan,
    DefinitionBatchSourceHash, DefinitionOperation, PlannedDefinition, PlannedLifecycleTransition,
};
pub use recovery::{
    CatalogDurableMutationPayload, CatalogRecoveredBatch, CatalogRecoveryAnomaly,
    CatalogRecoveryAnomalyKind, CatalogRecoveryOutcome, CatalogRecoveryReport, CatalogSkippedBatch,
    CatalogSkippedBatchReason, recover_catalog_snapshot_from_durable_payloads,
    replay_catalog_mutation_records,
};
pub use server::{
    CatalogChangeNotification, CatalogChangeSubscription, CatalogChangeSubscriptionCursor,
    CatalogManifestRecord, CatalogManifestResolution, CatalogManifestResolutionFailure,
    CatalogManifestResolutionRequest, CatalogManifestResolutionStatus,
    CatalogManifestRuntimeMetadata, CatalogManifestSelector, CatalogManifestStore,
    CatalogManifestStoreBoundary, CatalogRuntimeEvidence, CatalogRuntimeReopenEvidence,
    CatalogRuntimeStore, CatalogServerRuntime, CatalogServerRuntimeDiagnostic,
    CatalogServerRuntimeKind, CatalogServerTrait, CatalogSnapshotManifestStore,
    CatalogSubscriptionRegistry, ColumnSchema, DurableCatalogRuntimeHandle, ProcedureManifest,
    require_durable_catalog_runtime,
};
pub use snapshot::{CatalogSnapshot, CatalogSnapshotPublication};
pub use store::{
    CatalogSystemApplyReport, CatalogSystemDurableApplyReport, CatalogSystemStore,
    CatalogSystemWalAppend,
};
