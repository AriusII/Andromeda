//! Catalog mutation records, plans, and WAL-boundary types.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, DatabaseId, NamespaceId,
};
use std::collections::BTreeSet;

use crate::{CatalogObjectRef, objects::CatalogDefinition};

use super::definition::{CatalogLifecycleTarget, DefinitionBatchId};

/// Version-advancement proof for a single catalog mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutation {
    pub definition_batch_id: DefinitionBatchId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
}

impl CatalogMutation {
    /// Returns `true` when `next_version` strictly advances beyond `previous_version`.
    pub fn is_monotonic(self) -> bool {
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

/// Discriminant for a [`CatalogMutationRecord`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogMutationRecordKind {
    CatalogChangeBegin,
    CatalogChangeApply,
    CatalogChangeCommit,
}

/// The boundary markers written at the start and end of a mutation's WAL span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutationBoundary {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub publication_semantics: CatalogPublicationSemantics,
}

/// One atomic object-level change within a [`CatalogMutationPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMutationDelta {
    pub operation_index: usize,
    pub planned_version: CatalogVersion,
    pub operation: CatalogMutationOperation,
}

impl CatalogMutationDelta {
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
        target: CatalogLifecycleTarget,
    ) -> Self {
        Self {
            operation_index,
            planned_version,
            operation: CatalogMutationOperation::DeprecateObject { target },
        }
    }

    pub fn object(&self) -> &CatalogObjectRef {
        match &self.operation {
            CatalogMutationOperation::CreateObject { object, .. } => object,
            CatalogMutationOperation::DeprecateObject { target } => &target.object,
        }
    }

    pub fn definition(&self) -> Option<&CatalogDefinition> {
        match &self.operation {
            CatalogMutationOperation::CreateObject { definition, .. } => Some(definition),
            CatalogMutationOperation::DeprecateObject { .. } => None,
        }
    }
}

/// The specific operation carried by a [`CatalogMutationDelta`].
#[allow(
    clippy::large_enum_variant,
    reason = "Catalog WAL mutation payloads stay inline to preserve deterministic value semantics."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationOperation {
    CreateObject {
        object: CatalogObjectRef,
        definition: CatalogDefinition,
    },
    DeprecateObject {
        target: CatalogLifecycleTarget,
    },
}

/// A WAL record emitted during catalog mutation replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationRecord {
    Begin(CatalogMutationBoundary),
    Apply(Box<CatalogMutationDelta>),
    Commit(CatalogMutationBoundary),
}

impl CatalogMutationRecord {
    pub fn kind(&self) -> CatalogMutationRecordKind {
        match self {
            Self::Begin(_) => CatalogMutationRecordKind::CatalogChangeBegin,
            Self::Apply(_) => CatalogMutationRecordKind::CatalogChangeApply,
            Self::Commit(_) => CatalogMutationRecordKind::CatalogChangeCommit,
        }
    }
}

/// The ordered, validated set of deltas that will be applied atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMutationPlan {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub publication_semantics: CatalogPublicationSemantics,
    pub deltas: Vec<CatalogMutationDelta>,
}

impl CatalogMutationPlan {
    pub fn new(
        batch_id: DefinitionBatchId,
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        previous_version: CatalogVersion,
        next_version: CatalogVersion,
        deltas: Vec<CatalogMutationDelta>,
    ) -> AndromedaResult<Self> {
        if deltas.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan must contain at least one delta",
            ));
        }

        let mutation = CatalogMutation {
            definition_batch_id: batch_id,
            previous_version,
            next_version,
        };
        if !mutation.is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan must advance the catalog version",
            ));
        }

        let mut object_ids = BTreeSet::new();
        let mut object_names = BTreeSet::new();
        for (expected_index, delta) in deltas.iter().enumerate() {
            if delta.operation_index != expected_index {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation deltas must be dense and operation ordered",
                ));
            }

            if delta.planned_version != next_version {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation delta version must match the planned next catalog version",
                ));
            }

            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, definition } => {
                    definition.validate()?;
                    if object.catalog_version != next_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation object version must match the planned next catalog version",
                        ));
                    }

                    if !object_ids.insert(object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object id twice",
                        ));
                    }

                    if !object_names.insert(object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }
                }
                CatalogMutationOperation::DeprecateObject { target } => {
                    target.validate()?;
                    if target.object.catalog_version > previous_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation lifecycle target version must not be newer than the previous catalog version",
                        ));
                    }

                    if !object_ids.insert(target.object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object id twice",
                        ));
                    }

                    if !object_names.insert(target.object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }
                }
            }
        }

        Ok(Self {
            batch_id,
            database_id,
            namespace_id,
            previous_version,
            next_version,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
            deltas,
        })
    }

    /// Emits the full ordered sequence of WAL records for this plan.
    pub fn records(&self) -> Vec<CatalogMutationRecord> {
        let mut records = Vec::with_capacity(self.deltas.len() + 2);
        let boundary = self.commit_boundary();

        records.push(CatalogMutationRecord::Begin(boundary));
        records.extend(
            self.deltas
                .iter()
                .cloned()
                .map(|delta| CatalogMutationRecord::Apply(Box::new(delta))),
        );
        records.push(CatalogMutationRecord::Commit(boundary));
        records
    }

    /// Returns the total number of WAL records this plan emits (deltas + Begin + Commit).
    pub fn record_count(&self) -> usize {
        self.deltas.len() + 2
    }

    /// Returns the [`CatalogMutationBoundary`] shared by the Begin and Commit records.
    pub fn commit_boundary(&self) -> CatalogMutationBoundary {
        CatalogMutationBoundary {
            batch_id: self.batch_id,
            database_id: self.database_id,
            namespace_id: self.namespace_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
            publication_semantics: self.publication_semantics,
        }
    }

    /// Returns the [`CatalogMutation`] identity for this plan.
    pub fn mutation(&self) -> CatalogMutation {
        CatalogMutation {
            definition_batch_id: self.batch_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
        }
    }
}
