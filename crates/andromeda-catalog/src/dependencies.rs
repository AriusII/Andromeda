//! Catalog dependency extraction and validation helpers.
//!
//! The catalog dependency model is intentionally derived from the canonical
//! object definitions.  It does not add a second durable dependency store; it
//! provides focused dependency edges that batch and snapshot planners can use
//! consistently while the object model grows.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    batch::DefinitionOperation, names::QualifiedName, objects::CatalogDefinition,
    objects::ObjectKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CatalogDependencyKind {
    ProcedureStructuredInput,
}

impl CatalogDependencyKind {
    pub fn dependent_kind(self) -> ObjectKind {
        match self {
            Self::ProcedureStructuredInput => ObjectKind::Procedure,
        }
    }

    pub fn dependency_kind(self) -> ObjectKind {
        match self {
            Self::ProcedureStructuredInput => ObjectKind::StructuredObject,
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
        let kind = CatalogDependencyKind::ProcedureStructuredInput;
        Self {
            kind,
            dependent_name,
            dependent_kind: kind.dependent_kind(),
            dependency_name,
            dependency_kind: kind.dependency_kind(),
        }
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
                dependency.validate()?;

                let Some(dependency_index) =
                    created_name_positions.get(&dependency.dependency_name)
                else {
                    continue;
                };

                if created_name_kinds.get(&dependency.dependency_name)
                    != Some(&dependency.dependency_kind)
                {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog dependency must reference an object of the expected kind",
                    ));
                }

                graph
                    .edges
                    .entry(dependency.dependency_name.clone())
                    .or_default()
                    .insert(dependent.name.clone());

                if *dependency_index >= operation_index {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "definition batch must create intra-batch dependencies before dependent objects",
                    ));
                }
            }
        }

        graph.validate_acyclic()?;
        Ok(graph)
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
