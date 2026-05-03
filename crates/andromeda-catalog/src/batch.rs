use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    DatabaseId, NamespaceId,
};
use std::collections::BTreeSet;

use crate::{ObjectKind, QualifiedName, objects::CatalogDefinition};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionOperation {
    Create(CatalogDefinition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatch {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub base_version: CatalogVersion,
    pub operations: Vec<DefinitionOperation>,
}

impl DefinitionBatch {
    pub fn dry_run(&self) -> AndromedaResult<DefinitionBatchPlan> {
        if self.operations.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch must contain at least one operation",
            ));
        }

        let mut object_ids = BTreeSet::new();
        let mut object_names = BTreeSet::new();
        let mut created_objects = Vec::with_capacity(self.operations.len());

        for operation in &self.operations {
            match operation {
                DefinitionOperation::Create(definition) => {
                    definition.validate()?;

                    let object = definition.object_ref();
                    if !object_ids.insert(object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch must not create the same object id twice",
                        ));
                    }

                    if !object_names.insert(object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch must not create the same object name twice",
                        ));
                    }

                    created_objects.push(PlannedDefinition {
                        object_id: object.object_id,
                        name: object.name.clone(),
                        kind: object.kind,
                    });
                }
            }
        }

        let next_version = self.base_version.get().checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog version overflow during definition batch planning",
            )
        })?;

        Ok(DefinitionBatchPlan {
            batch_id: self.batch_id,
            operation_count: self.operations.len(),
            previous_version: self.base_version,
            next_version: CatalogVersion::new(next_version),
            created_objects,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDefinition {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchPlan {
    pub batch_id: DefinitionBatchId,
    pub operation_count: usize,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub created_objects: Vec<PlannedDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutation {
    pub definition_batch_id: DefinitionBatchId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
}

impl CatalogMutation {
    pub fn is_monotonic(self) -> bool {
        self.next_version.get() > self.previous_version.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CatalogObjectRef, ObjectKind, QualifiedName, TableDefinition};
    use andromeda_core::{CatalogObjectId, ColumnDescriptor, ScalarType, TypeDescriptor};

    fn object(kind: ObjectKind) -> CatalogObjectRef {
        object_with(1, "Inventory.Product", kind)
    }

    fn object_with(id: u64, name: &str, kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: CatalogVersion::new(7),
        }
    }

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    #[test]
    fn definition_batch_dry_run_validates_and_advances_catalog_version() {
        let table = TableDefinition {
            object: object(ObjectKind::Table),
            columns: vec![column("ProductId", 0)],
        };
        let batch = DefinitionBatch {
            batch_id: DefinitionBatchId::new(4),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(10),
            operations: vec![DefinitionOperation::Create(CatalogDefinition::Table(table))],
        };

        let plan = batch.dry_run().unwrap();

        assert_eq!(plan.operation_count, 1);
        assert_eq!(plan.next_version, CatalogVersion::new(11));
        assert_eq!(plan.created_objects.len(), 1);
        assert_eq!(plan.created_objects[0].kind, ObjectKind::Table);
    }

    #[test]
    fn definition_batch_dry_run_rejects_duplicate_creation_ids() {
        let first = TableDefinition {
            object: object_with(1, "Inventory.Product", ObjectKind::Table),
            columns: vec![column("ProductId", 0)],
        };
        let second = TableDefinition {
            object: object_with(1, "Inventory.Stock", ObjectKind::Table),
            columns: vec![column("StockId", 0)],
        };
        let batch = DefinitionBatch {
            batch_id: DefinitionBatchId::new(4),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(10),
            operations: vec![
                DefinitionOperation::Create(CatalogDefinition::Table(first)),
                DefinitionOperation::Create(CatalogDefinition::Table(second)),
            ],
        };

        let error = batch.dry_run().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("object id"));
    }

    #[test]
    fn definition_batch_dry_run_rejects_duplicate_creation_names() {
        let first = TableDefinition {
            object: object_with(1, "Inventory.Product", ObjectKind::Table),
            columns: vec![column("ProductId", 0)],
        };
        let second = TableDefinition {
            object: object_with(2, "Inventory.Product", ObjectKind::Table),
            columns: vec![column("StockId", 0)],
        };
        let batch = DefinitionBatch {
            batch_id: DefinitionBatchId::new(4),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(10),
            operations: vec![
                DefinitionOperation::Create(CatalogDefinition::Table(first)),
                DefinitionOperation::Create(CatalogDefinition::Table(second)),
            ],
        };

        let error = batch.dry_run().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("object name"));
    }

    #[test]
    fn definition_batch_dry_run_rejects_mismatched_definition_kind() {
        let table = TableDefinition {
            object: object(ObjectKind::Procedure),
            columns: vec![column("ProductId", 0)],
        };
        let batch = DefinitionBatch {
            batch_id: DefinitionBatchId::new(4),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(10),
            operations: vec![DefinitionOperation::Create(CatalogDefinition::Table(table))],
        };

        let error = batch.dry_run().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("kind"));
    }

    #[test]
    fn catalog_mutation_must_advance_version() {
        let mutation = CatalogMutation {
            definition_batch_id: DefinitionBatchId::new(1),
            previous_version: CatalogVersion::new(10),
            next_version: CatalogVersion::new(11),
        };

        assert!(mutation.is_monotonic());

        let stale = CatalogMutation {
            next_version: CatalogVersion::new(10),
            ..mutation
        };

        assert!(!stale.is_monotonic());
    }
}
