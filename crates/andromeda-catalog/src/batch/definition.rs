//! Core definition batch types: identifiers, operations, and the batch itself.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, DatabaseId, NamespaceId,
};
use std::collections::BTreeSet;

use crate::{
    dependencies::validate_in_batch_dependencies, objects::CatalogDefinition, CatalogObjectRef,
};

use super::mutation::{CatalogMutation, CatalogMutationDelta, CatalogMutationPlan};
use super::plan::{
    CatalogLifecycleAction, DefinitionBatchPlan, PlannedDefinition, PlannedLifecycleTransition,
};

/// Unique identifier for a [`DefinitionBatch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchId(u64);

impl DefinitionBatchId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A single operation within a [`DefinitionBatch`].
#[allow(
    clippy::large_enum_variant,
    reason = "DefinitionBatch keeps catalog definitions inline for stable public contract shape."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionOperation {
    Create(CatalogDefinition),
    Deprecate(CatalogLifecycleTarget),
}

/// Identifies the catalog object targeted by a lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLifecycleTarget {
    pub object: CatalogObjectRef,
}

impl CatalogLifecycleTarget {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)
    }
}

/// An ordered, atomic set of catalog object definition operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatch {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub base_version: CatalogVersion,
    pub operations: Vec<DefinitionOperation>,
}

impl DefinitionBatch {
    /// Validates all operations and produces a [`DefinitionBatchPlan`] without
    /// applying any mutations.
    pub fn dry_run(&self) -> AndromedaResult<DefinitionBatchPlan> {
        if self.operations.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch must contain at least one operation",
            ));
        }

        if self.batch_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch id must not be zero",
            ));
        }

        if self.database_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch database id must not be zero",
            ));
        }

        if self.namespace_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch namespace id must not be zero",
            ));
        }

        let next_version = self.base_version.get().checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog version overflow during definition batch planning",
            )
        })?;
        let next_version = CatalogVersion::new(next_version);

        let mut object_ids = BTreeSet::new();
        let mut object_names = BTreeSet::new();
        let mut lifecycle_target_ids = BTreeSet::new();
        let mut lifecycle_target_names = BTreeSet::new();
        let mut created_objects = Vec::with_capacity(self.operations.len());
        let mut deprecated_objects = Vec::new();
        let mut deltas = Vec::with_capacity(self.operations.len());

        for (operation_index, operation) in self.operations.iter().enumerate() {
            match operation {
                DefinitionOperation::Create(definition) => {
                    definition.validate()?;

                    let object = definition.object_ref();
                    if object.catalog_version != next_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "created object catalog version must match the planned next catalog version",
                        ));
                    }

                    if lifecycle_target_ids.contains(&object.object_id)
                        || !object_ids.insert(object.object_id)
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch must not change the same lifecycle object id twice",
                        ));
                    }

                    if lifecycle_target_names.contains(&object.name)
                        || !object_names.insert(object.name.clone())
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch must not change the same lifecycle object name twice",
                        ));
                    }

                    created_objects.push(PlannedDefinition {
                        object_id: object.object_id,
                        name: object.name.clone(),
                        kind: object.kind,
                        planned_version: next_version,
                    });
                    deltas.push(CatalogMutationDelta::create(
                        operation_index,
                        next_version,
                        definition.clone(),
                    ));
                }
                DefinitionOperation::Deprecate(target) => {
                    target.validate()?;

                    if target.object.catalog_version > self.base_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "deprecated object version must not be newer than the definition batch base version",
                        ));
                    }

                    if object_ids.contains(&target.object.object_id)
                        || !lifecycle_target_ids.insert(target.object.object_id)
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch must not change the same lifecycle object id twice",
                        ));
                    }

                    if object_names.contains(&target.object.name)
                        || !lifecycle_target_names.insert(target.object.name.clone())
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch must not change the same lifecycle object name twice",
                        ));
                    }

                    deprecated_objects.push(PlannedLifecycleTransition {
                        object_id: target.object.object_id,
                        name: target.object.name.clone(),
                        kind: target.object.kind,
                        action: CatalogLifecycleAction::Deprecate,
                        planned_version: next_version,
                    });
                    deltas.push(CatalogMutationDelta::deprecate(
                        operation_index,
                        next_version,
                        target.clone(),
                    ));
                }
            }
        }

        validate_in_batch_dependencies(&self.operations)?;

        let mutation = CatalogMutation {
            definition_batch_id: self.batch_id,
            previous_version: self.base_version,
            next_version,
        };
        if !mutation.is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation must advance the catalog version",
            ));
        }

        let mutation_plan = CatalogMutationPlan::new(
            self.batch_id,
            self.database_id,
            self.namespace_id,
            self.base_version,
            next_version,
            deltas,
        )?;

        Ok(DefinitionBatchPlan {
            batch_id: self.batch_id,
            database_id: self.database_id,
            namespace_id: self.namespace_id,
            operation_count: self.operations.len(),
            previous_version: self.base_version,
            next_version,
            created_objects,
            deprecated_objects,
            mutation_plan,
        })
    }
}
