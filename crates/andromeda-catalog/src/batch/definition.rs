//! Core definition batch types: identifiers, operations, and the batch itself.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};
use std::collections::BTreeSet;

use crate::{
    CatalogObjectRef,
    dependencies::{DefinitionBatchDependencyGraphHash, validate_in_batch_dependencies},
    digest::Sha256,
    objects::{CatalogDefinition, ObjectKind},
};

use super::dry_run_srpl::validate_srpl_batch_dry_run;
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

/// Canonical SHA-256 digest of the ordered DefinitionBatch source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchSourceHash([u8; Self::LEN]);

impl DefinitionBatchSourceHash {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
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
    /// Computes a deterministic digest over the exact ordered batch source.
    ///
    /// This hash intentionally includes operation order and object shape
    /// hashes. It complements [`DefinitionBatchDependencyGraphHash`], which
    /// canonicalizes the dependency graph and therefore ignores source order
    /// when the graph itself is equivalent.
    pub fn source_hash(&self) -> DefinitionBatchSourceHash {
        let mut sink = DefinitionBatchSourceHashSink::new();
        sink.str("andromeda.catalog.definition-batch.source.v1.sha256");
        sink.u64(self.batch_id.get());
        sink.u64(self.database_id.get());
        sink.u64(self.namespace_id.get());
        sink.u64(self.base_version.get());
        sink.u64(self.operations.len() as u64);
        for (operation_index, operation) in self.operations.iter().enumerate() {
            sink.u64(operation_index as u64);
            match operation {
                DefinitionOperation::Create(definition) => {
                    sink.u8(0);
                    sink.object_ref(definition.object_ref());
                    sink.raw_bytes(&definition.shape_hash().as_bytes());
                }
                DefinitionOperation::Deprecate(target) => {
                    sink.u8(1);
                    sink.object_ref(&target.object);
                }
            }
        }
        sink.finish()
    }

    /// Validates and computes the canonical dependency-graph digest for this
    /// batch.
    pub fn dependency_graph_hash(&self) -> AndromedaResult<DefinitionBatchDependencyGraphHash> {
        validate_in_batch_dependencies(&self.operations).map(|graph| graph.dependency_graph_hash())
    }

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
        validate_srpl_batch_dry_run(self)?;

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

        let source_hash = self.source_hash();
        let dependency_graph_hash =
            validate_in_batch_dependencies(&self.operations)?.dependency_graph_hash();

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
            source_hash,
            dependency_graph_hash,
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

struct DefinitionBatchSourceHashSink {
    hasher: Sha256,
}

impl DefinitionBatchSourceHashSink {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> DefinitionBatchSourceHash {
        DefinitionBatchSourceHash::new(self.hasher.finalize())
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update(&[value]);
    }

    fn u64(&mut self, value: u64) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn object_ref(&mut self, object: &CatalogObjectRef) {
        self.u64(object.object_id.get());
        self.qualified_name(&object.name);
        self.object_kind(object.kind);
        self.u64(object.catalog_version.get());
    }

    fn qualified_name(&mut self, name: &crate::QualifiedName) {
        self.u64(name.parts().len() as u64);
        for part in name.parts() {
            self.str(part);
        }
    }

    fn object_kind(&mut self, kind: ObjectKind) {
        self.u8(match kind {
            ObjectKind::Database => 0,
            ObjectKind::Namespace => 1,
            ObjectKind::Table => 2,
            ObjectKind::Map => 3,
            ObjectKind::Enum => 4,
            ObjectKind::StructuredObject => 5,
            ObjectKind::Procedure => 6,
        });
    }
}
