//! Catalog dependency extraction and validation helpers.
//!
//! The catalog dependency model is intentionally derived from the canonical
//! object definitions.  It does not add a second durable dependency store; it
//! provides focused dependency edges that batch and snapshot planners can use
//! consistently while the object model grows.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    batch::DefinitionOperation,
    names::QualifiedName,
    objects::{CatalogDefinition, CatalogObjectBinding, ObjectKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CatalogDependencyKind {
    /// A procedure consumes a structured object as one of its structured
    /// inputs.  Derived directly from `ProcedureContract::structured_inputs`.
    ProcedureStructuredInput,
    /// A procedure reads from a table at execution time.  Derived from
    /// catalog object bindings (`CatalogBindingKind::ReadsTable`) supplied
    /// alongside the definition batch.
    ProcedureReadsTable,
    /// A procedure writes to a table at execution time.  Derived from
    /// catalog object bindings (`CatalogBindingKind::WritesTable`).
    ProcedureWritesTable,
    /// A procedure emits a structured-object result as one of its result
    /// streams.  Derived from `CatalogBindingKind::EmitsStructuredObject`.
    ProcedureEmitsStructuredObject,
}

impl CatalogDependencyKind {
    pub fn dependent_kind(self) -> ObjectKind {
        match self {
            Self::ProcedureStructuredInput
            | Self::ProcedureReadsTable
            | Self::ProcedureWritesTable
            | Self::ProcedureEmitsStructuredObject => ObjectKind::Procedure,
        }
    }

    pub fn dependency_kind(self) -> ObjectKind {
        match self {
            Self::ProcedureStructuredInput | Self::ProcedureEmitsStructuredObject => {
                ObjectKind::StructuredObject
            }
            Self::ProcedureReadsTable | Self::ProcedureWritesTable => ObjectKind::Table,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDependency {
    pub kind: CatalogDependencyKind,
    pub dependent_name: QualifiedName,
    pub dependent_kind: ObjectKind,
    pub dependency_name: QualifiedName,
    pub dependency_kind: ObjectKind,
}

impl CatalogDependency {
    pub fn procedure_structured_input(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureStructuredInput,
            dependent_name,
            dependency_name,
        )
    }

    pub fn procedure_reads_table(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureReadsTable,
            dependent_name,
            dependency_name,
        )
    }

    pub fn procedure_writes_table(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureWritesTable,
            dependent_name,
            dependency_name,
        )
    }

    pub fn procedure_emits_structured_object(
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self::with_kind(
            CatalogDependencyKind::ProcedureEmitsStructuredObject,
            dependent_name,
            dependency_name,
        )
    }

    fn with_kind(
        kind: CatalogDependencyKind,
        dependent_name: QualifiedName,
        dependency_name: QualifiedName,
    ) -> Self {
        Self {
            kind,
            dependent_name,
            dependent_kind: kind.dependent_kind(),
            dependency_name,
            dependency_kind: kind.dependency_kind(),
        }
    }

    /// Lower a `CatalogObjectBinding` (the runtime evidence emitted by the
    /// fixtures and by the SRPL binder) into a structural catalog
    /// dependency edge.  The binding is validated by the caller; this
    /// helper only translates between the two representations.
    pub fn from_binding(binding: &CatalogObjectBinding) -> Self {
        use crate::objects::CatalogBindingKind;
        let kind = match binding.kind {
            CatalogBindingKind::ReadsTable => CatalogDependencyKind::ProcedureReadsTable,
            CatalogBindingKind::WritesTable => CatalogDependencyKind::ProcedureWritesTable,
            CatalogBindingKind::UsesStructuredInput => {
                CatalogDependencyKind::ProcedureStructuredInput
            }
            CatalogBindingKind::EmitsStructuredObject => {
                CatalogDependencyKind::ProcedureEmitsStructuredObject
            }
        };
        Self::with_kind(
            kind,
            binding.dependent.name.clone(),
            binding.dependency.name.clone(),
        )
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.dependent_kind != self.kind.dependent_kind() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency dependent kind must match dependency kind",
            ));
        }

        if self.dependency_kind != self.kind.dependency_kind() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency dependency kind must match dependency kind",
            ));
        }

        if self.dependent_name == self.dependency_name {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog dependency dependent and dependency names must differ",
            ));
        }

        Ok(())
    }
}

impl CatalogDefinition {
    pub fn dependencies(&self) -> Vec<CatalogDependency> {
        match self {
            Self::Procedure(procedure) => procedure
                .structured_inputs
                .iter()
                .map(|structured_input| {
                    CatalogDependency::procedure_structured_input(
                        procedure.object.name.clone(),
                        structured_input.clone(),
                    )
                })
                .collect(),
            Self::Table(_) | Self::StructuredObject(_) | Self::Enum(_) => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchDependencyGraph {
    edges: BTreeMap<QualifiedName, BTreeSet<QualifiedName>>,
}

impl BatchDependencyGraph {
    pub fn from_operations(operations: &[DefinitionOperation]) -> AndromedaResult<Self> {
        Self::from_operations_with_bindings(operations, &[])
    }

    /// Build the dependency graph for a definition batch, augmenting the
    /// intra-batch edges derived from definition shapes with externally
    /// supplied catalog bindings.  Bindings whose dependent procedure is part
    /// of this batch contribute additional Procedure → {Table, Structured
    /// Object} edges; bindings that reference objects outside this batch are
    /// recorded as dependents of the batch's own nodes (so cycle detection
    /// still applies) but are not required to also appear inside the batch.
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

            // Bindings only contribute to the batch graph when the dependent
            // procedure is created inside this batch; otherwise the binding
            // describes an out-of-batch relationship that is enforced by the
            // catalog store, not by definition-batch validation.
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

    fn absorb_dependency(
        &mut self,
        dependent_name: &QualifiedName,
        dependency: &CatalogDependency,
        operation_index: usize,
        created_name_positions: &BTreeMap<QualifiedName, usize>,
        created_name_kinds: &BTreeMap<QualifiedName, ObjectKind>,
    ) -> AndromedaResult<()> {
        dependency.validate()?;

        let Some(dependency_index) = created_name_positions.get(&dependency.dependency_name) else {
            // Out-of-batch dependency: nothing to validate against the graph,
            // but the validation above already checked it is structurally
            // well-formed.
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
