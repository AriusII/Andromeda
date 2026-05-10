//! Portable catalog mutation DTOs owned by the catalog-store boundary.
//!
//! These types carry catalog mutation identity, object deltas, boundary
//! metadata, and publication intent without owning WAL codecs or storage I/O.

use andromeda_contract::CatalogDefinition;
use andromeda_error::AndromedaResult;
use andromeda_procedure_contract::CatalogObjectRef;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

/// Maximum number of Apply records that one durable DefinitionBatch replay may carry.
pub const CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH: usize = 1024;

/// Minimal behavior required from a catalog lifecycle target carried in a
/// portable mutation delta.
pub trait CatalogLifecycleMutationTarget {
    fn validate_for_catalog_mutation(&self) -> AndromedaResult<()>;

    fn object_ref(&self) -> &CatalogObjectRef;
}

impl CatalogLifecycleMutationTarget for CatalogObjectRef {
    fn validate_for_catalog_mutation(&self) -> AndromedaResult<()> {
        self.validate_for_definition(self.kind)
    }

    fn object_ref(&self) -> &CatalogObjectRef {
        self
    }
}

/// Version-advancement proof for a single catalog mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutation<DefinitionBatchId> {
    pub definition_batch_id: DefinitionBatchId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
}

impl<DefinitionBatchId> CatalogMutation<DefinitionBatchId> {
    /// Returns `true` when `next_version` strictly advances beyond `previous_version`.
    pub fn is_monotonic(&self) -> bool {
        self.next_version.get() > self.previous_version.get()
    }
}

/// Controls whether a mutation is acknowledged only within the engine or also
/// confirmed to an external durable store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationSemantics {
    PlannedVersionOnly,
    DurablePublicationExternal,
}

/// The boundary markers written at the start and end of a mutation's WAL span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutationBoundary<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
> {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub source_hash: DefinitionBatchSourceHash,
    pub dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub expected_apply_count: usize,
    pub publication_semantics: CatalogPublicationSemantics,
}

/// One atomic object-level change within a catalog mutation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMutationDelta<LifecycleTarget> {
    pub operation_index: usize,
    pub planned_version: CatalogVersion,
    pub operation: CatalogMutationOperation<LifecycleTarget>,
}

impl<LifecycleTarget> CatalogMutationDelta<LifecycleTarget> {
    pub fn create(
        operation_index: usize,
        planned_version: CatalogVersion,
        definition: CatalogDefinition,
    ) -> Self {
        let object = definition.object_ref().clone();
        Self {
            operation_index,
            planned_version,
            operation: CatalogMutationOperation::CreateObject { object, definition },
        }
    }

    pub fn deprecate(
        operation_index: usize,
        planned_version: CatalogVersion,
        target: LifecycleTarget,
    ) -> Self {
        Self {
            operation_index,
            planned_version,
            operation: CatalogMutationOperation::DeprecateObject { target },
        }
    }

    pub fn definition(&self) -> Option<&CatalogDefinition> {
        match &self.operation {
            CatalogMutationOperation::CreateObject { definition, .. } => Some(definition),
            CatalogMutationOperation::DeprecateObject { .. } => None,
        }
    }
}

impl<LifecycleTarget> CatalogMutationDelta<LifecycleTarget>
where
    LifecycleTarget: CatalogLifecycleMutationTarget,
{
    pub fn object(&self) -> &CatalogObjectRef {
        match &self.operation {
            CatalogMutationOperation::CreateObject { object, .. } => object,
            CatalogMutationOperation::DeprecateObject { target } => target.object_ref(),
        }
    }
}

/// The specific operation carried by a [`CatalogMutationDelta`].
#[allow(
    clippy::large_enum_variant,
    reason = "Catalog WAL mutation payloads stay inline to preserve deterministic value semantics."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationOperation<LifecycleTarget> {
    CreateObject {
        object: CatalogObjectRef,
        definition: CatalogDefinition,
    },
    /// Marks an existing object inactive while retaining its historical
    /// identity. `DeprecateObject` is stable catalog lifecycle terminology, not
    /// a deprecation marker for this API.
    DeprecateObject { target: LifecycleTarget },
}
