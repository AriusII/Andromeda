//! DefinitionBatch dependency extraction and validation helpers.

pub use andromeda_contract::{CatalogDependency, CatalogDependencyKind};
use andromeda_contract::{CatalogObjectBinding, ObjectKind, QualifiedName};
use andromeda_digest::Sha256;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::{BTreeMap, BTreeSet};

use crate::{DefinitionOperation, operation::object_kind_tag};

/// Canonical SHA-256 digest of a DefinitionBatch dependency graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DefinitionBatchDependencyGraphHash([u8; Self::LEN]);

impl DefinitionBatchDependencyGraphHash {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchDependencyGraph {
    edges: BTreeMap<QualifiedName, BTreeSet<QualifiedName>>,
    dependencies: Vec<CatalogDependency>,
}

impl BatchDependencyGraph {
    pub fn from_operations(operations: &[DefinitionOperation]) -> AndromedaResult<Self> {
        Self::from_operations_with_bindings(operations, &[])
    }

    /// Build the dependency graph for a definition batch, augmenting the
    /// intra-batch edges derived from definition shapes with externally
    /// supplied catalog bindings.
    pub fn from_operations_with_bindings(
        operations: &[DefinitionOperation],
        bindings: &[CatalogObjectBinding],
    ) -> AndromedaResult<Self> {
        let mut created_name_positions = BTreeMap::new();
        let mut created_name_kinds = BTreeMap::new();

        for (operation_index, operation) in operations.iter().enumerate() {
            let DefinitionOperation::Create(definition) = operation else {
                continue;
            };
            let object = definition.object_ref();
            created_name_positions.insert(object.name.clone(), operation_index);
            created_name_kinds.insert(object.name.clone(), object.kind);
        }

        let mut graph = Self {
            edges: BTreeMap::new(),
            dependencies: Vec::new(),
        };

        for (operation_index, operation) in operations.iter().enumerate() {
            let DefinitionOperation::Create(definition) = operation else {
                continue;
            };

            let dependent = definition.object_ref();
            graph.edges.entry(dependent.name.clone()).or_default();

            for dependency in definition.dependencies() {
                graph.absorb_dependency(
                    &dependent.name,
                    &dependency,
                    operation_index,
                    &created_name_positions,
                    &created_name_kinds,
                )?;
            }
        }

        for binding in bindings {
            binding.validate()?;
            let dependency = CatalogDependency::from_binding(binding);

            let Some(operation_index) = created_name_positions.get(&dependency.dependent_name)
            else {
                continue;
            };

            graph.absorb_dependency(
                &dependency.dependent_name,
                &dependency,
                *operation_index,
                &created_name_positions,
                &created_name_kinds,
            )?;
        }

        graph.validate_acyclic()?;
        Ok(graph)
    }

    /// Returns a deterministic digest over graph nodes and canonical dependency
    /// edges.
    pub fn dependency_graph_hash(&self) -> DefinitionBatchDependencyGraphHash {
        let mut sink = DependencyGraphHashSink::new();
        sink.str("andromeda.catalog.definition-batch.dependency-graph.v1.sha256");

        sink.u64(self.edges.len() as u64);
        for node in self.edges.keys() {
            sink.qualified_name(node);
        }

        let dependencies = self.canonical_dependencies();
        sink.u64(dependencies.len() as u64);
        for dependency in dependencies {
            sink.dependency(&dependency);
        }

        sink.finish()
    }

    /// Returns dependency edges in canonical order, with duplicate graph edges
    /// collapsed. Source-level duplicates remain visible through
    /// `DefinitionBatchSourceHash`.
    pub fn canonical_dependencies(&self) -> Vec<CatalogDependency> {
        let mut dependencies = self.dependencies.clone();
        dependencies.sort_by_key(canonical_dependency_key);
        dependencies.dedup_by(|left, right| {
            canonical_dependency_key(left) == canonical_dependency_key(right)
        });
        dependencies
    }

    pub fn dependency_count(&self) -> usize {
        self.canonical_dependencies().len()
    }

    fn absorb_dependency(
        &mut self,
        dependent_name: &QualifiedName,
        dependency: &CatalogDependency,
        operation_index: usize,
        created_name_positions: &BTreeMap<QualifiedName, usize>,
        created_name_kinds: &BTreeMap<QualifiedName, ObjectKind>,
    ) -> AndromedaResult<()> {
        dependency.validate()?;
        self.dependencies.push(dependency.clone());

        let Some(dependency_index) = created_name_positions.get(&dependency.dependency_name) else {
            return Ok(());
        };

        if created_name_kinds.get(&dependency.dependency_name) != Some(&dependency.dependency_kind)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency must reference an object of the expected kind",
            ));
        }

        self.edges
            .entry(dependency.dependency_name.clone())
            .or_default()
            .insert(dependent_name.clone());

        if *dependency_index >= operation_index {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch must create intra-batch dependencies before dependent objects",
            ));
        }

        Ok(())
    }

    pub fn validate_acyclic(&self) -> AndromedaResult<()> {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();

        for node in self.edges.keys() {
            self.visit(node, &mut visiting, &mut visited)?;
        }

        Ok(())
    }

    fn visit(
        &self,
        node: &QualifiedName,
        visiting: &mut BTreeSet<QualifiedName>,
        visited: &mut BTreeSet<QualifiedName>,
    ) -> AndromedaResult<()> {
        if visited.contains(node) {
            return Ok(());
        }

        if !visiting.insert(node.clone()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch catalog dependencies must be acyclic",
            ));
        }

        if let Some(dependents) = self.edges.get(node) {
            for dependent in dependents {
                self.visit(dependent, visiting, visited)?;
            }
        }

        visiting.remove(node);
        visited.insert(node.clone());
        Ok(())
    }
}

fn canonical_dependency_key(dependency: &CatalogDependency) -> (u8, String, u8, String, u8) {
    (
        dependency_kind_tag(dependency.kind),
        dependency.dependency_name.as_catalog_path(),
        object_kind_tag(dependency.dependency_kind),
        dependency.dependent_name.as_catalog_path(),
        object_kind_tag(dependency.dependent_kind),
    )
}

struct DependencyGraphHashSink {
    hasher: Sha256,
}

impl DependencyGraphHashSink {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> DefinitionBatchDependencyGraphHash {
        DefinitionBatchDependencyGraphHash::new(self.hasher.finalize())
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

    fn qualified_name(&mut self, name: &QualifiedName) {
        self.u64(name.parts().len() as u64);
        for part in name.parts() {
            self.str(part);
        }
    }

    fn dependency(&mut self, dependency: &CatalogDependency) {
        self.u8(dependency_kind_tag(dependency.kind));
        self.object_kind(dependency.dependent_kind);
        self.qualified_name(&dependency.dependent_name);
        self.object_kind(dependency.dependency_kind);
        self.qualified_name(&dependency.dependency_name);
    }

    fn object_kind(&mut self, kind: ObjectKind) {
        self.u8(object_kind_tag(kind));
    }
}

fn dependency_kind_tag(kind: CatalogDependencyKind) -> u8 {
    match kind {
        CatalogDependencyKind::ProcedureStructuredInput => 0,
        CatalogDependencyKind::ProcedureReadsTable => 1,
        CatalogDependencyKind::ProcedureWritesTable => 2,
        CatalogDependencyKind::ProcedureEmitsStructuredObject => 3,
    }
}

pub fn validate_in_batch_dependencies(
    operations: &[DefinitionOperation],
) -> AndromedaResult<BatchDependencyGraph> {
    BatchDependencyGraph::from_operations(operations)
}

pub fn validate_in_batch_dependencies_with_bindings(
    operations: &[DefinitionOperation],
    bindings: &[CatalogObjectBinding],
) -> AndromedaResult<BatchDependencyGraph> {
    BatchDependencyGraph::from_operations_with_bindings(operations, bindings)
}
