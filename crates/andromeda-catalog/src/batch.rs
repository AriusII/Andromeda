use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, DatabaseId, NamespaceId,
};

use crate::objects::CatalogDefinition;

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

        for operation in &self.operations {
            match operation {
                DefinitionOperation::Create(definition) => definition.validate()?,
            }
        }

        Ok(DefinitionBatchPlan {
            batch_id: self.batch_id,
            operation_count: self.operations.len(),
            previous_version: self.base_version,
            next_version: CatalogVersion::new(self.base_version.get() + 1),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionBatchPlan {
    pub batch_id: DefinitionBatchId,
    pub operation_count: usize,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
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
        CatalogObjectRef {
            object_id: CatalogObjectId::new(1),
            name: QualifiedName::parse("Inventory.Product").unwrap(),
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
