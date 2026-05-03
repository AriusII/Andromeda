use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    DatabaseId, NamespaceId,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::{CatalogObjectRef, ObjectKind, QualifiedName, objects::CatalogDefinition};

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
        let mut created_name_positions = BTreeMap::new();
        let mut created_name_kinds = BTreeMap::new();
        let mut created_objects = Vec::with_capacity(self.operations.len());
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

                    created_name_positions.insert(object.name.clone(), operation_index);
                    created_name_kinds.insert(object.name.clone(), object.kind);
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
            }
        }

        validate_dependency_order(
            &self.operations,
            &created_name_positions,
            &created_name_kinds,
        )?;

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
            mutation_plan,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDefinition {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub planned_version: CatalogVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchPlan {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub operation_count: usize,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub created_objects: Vec<PlannedDefinition>,
    pub mutation_plan: CatalogMutationPlan,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationSemantics {
    PlannedVersionOnly,
    DurablePublicationExternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogMutationRecordKind {
    CatalogChangeBegin,
    CatalogChangeApply,
    CatalogChangeCommit,
}

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

            delta.definition().validate()?;
            let object = delta.object();
            if object.catalog_version != next_version {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation object version must match the planned next catalog version",
                ));
            }

            if !object_ids.insert(object.object_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation plan must not create the same object id twice",
                ));
            }

            if !object_names.insert(object.name.clone()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation plan must not create the same object name twice",
                ));
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

    pub fn records(&self) -> Vec<CatalogMutationRecord> {
        let mut records = Vec::with_capacity(self.deltas.len() + 2);
        let boundary = CatalogMutationBoundary {
            batch_id: self.batch_id,
            database_id: self.database_id,
            namespace_id: self.namespace_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
            publication_semantics: self.publication_semantics,
        };

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

    pub fn mutation(&self) -> CatalogMutation {
        CatalogMutation {
            definition_batch_id: self.batch_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutationBoundary {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub publication_semantics: CatalogPublicationSemantics,
}

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

    pub fn object(&self) -> &CatalogObjectRef {
        match &self.operation {
            CatalogMutationOperation::CreateObject { object, .. } => object,
        }
    }

    pub fn definition(&self) -> &CatalogDefinition {
        match &self.operation {
            CatalogMutationOperation::CreateObject { definition, .. } => definition,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationOperation {
    CreateObject {
        object: CatalogObjectRef,
        definition: CatalogDefinition,
    },
}

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

fn validate_dependency_order(
    operations: &[DefinitionOperation],
    created_name_positions: &BTreeMap<QualifiedName, usize>,
    created_name_kinds: &BTreeMap<QualifiedName, ObjectKind>,
) -> AndromedaResult<()> {
    for (operation_index, operation) in operations.iter().enumerate() {
        let DefinitionOperation::Create(definition) = operation;
        let CatalogDefinition::Procedure(procedure) = definition else {
            continue;
        };

        for structured_input in &procedure.structured_inputs {
            let Some(dependency_index) = created_name_positions.get(structured_input) else {
                continue;
            };

            if *dependency_index >= operation_index {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch must create intra-batch dependencies before dependent objects",
                ));
            }

            if created_name_kinds.get(structured_input) != Some(&ObjectKind::StructuredObject) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "procedure structured input dependency must reference a structured object",
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, ProcedureContract,
        QualifiedName, StructuredObjectDefinition, TableDefinition, TransactionPolicy,
    };
    use andromeda_core::{
        CatalogObjectId, ColumnDescriptor, ContractHash, ProcedureId, ScalarType, TypeDescriptor,
    };

    fn object(kind: ObjectKind) -> CatalogObjectRef {
        object_with(1, "Inventory.Product", kind)
    }

    fn object_with(id: u64, name: &str, kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: CatalogVersion::new(11),
        }
    }

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn batch_with(operations: Vec<DefinitionOperation>) -> DefinitionBatch {
        DefinitionBatch {
            batch_id: DefinitionBatchId::new(4),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(10),
            operations,
        }
    }

    #[test]
    fn definition_batch_dry_run_validates_and_advances_catalog_version() {
        let table = TableDefinition {
            object: object(ObjectKind::Table),
            columns: vec![column("ProductId", 0)],
        };
        let batch = batch_with(vec![DefinitionOperation::Create(CatalogDefinition::Table(
            table,
        ))]);

        let plan = batch.dry_run().unwrap();

        assert_eq!(plan.operation_count, 1);
        assert_eq!(plan.next_version, CatalogVersion::new(11));
        assert_eq!(plan.created_objects.len(), 1);
        assert_eq!(plan.created_objects[0].kind, ObjectKind::Table);
        assert_eq!(
            plan.created_objects[0].planned_version,
            CatalogVersion::new(11)
        );
        assert_eq!(
            plan.mutation_plan.publication_semantics,
            CatalogPublicationSemantics::DurablePublicationExternal
        );
        assert_eq!(plan.mutation_plan.deltas.len(), 1);
        assert_eq!(plan.mutation_plan.deltas[0].operation_index, 0);
        assert_eq!(
            plan.mutation_plan.deltas[0].object().object_id,
            CatalogObjectId::new(1)
        );

        let records = plan.mutation_plan.records();
        assert_eq!(records.len(), 3);
        assert_eq!(
            records[0].kind(),
            CatalogMutationRecordKind::CatalogChangeBegin
        );
        assert_eq!(
            records[1].kind(),
            CatalogMutationRecordKind::CatalogChangeApply
        );
        assert_eq!(
            records[2].kind(),
            CatalogMutationRecordKind::CatalogChangeCommit
        );
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
        let batch = batch_with(vec![
            DefinitionOperation::Create(CatalogDefinition::Table(first)),
            DefinitionOperation::Create(CatalogDefinition::Table(second)),
        ]);

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
        let batch = batch_with(vec![
            DefinitionOperation::Create(CatalogDefinition::Table(first)),
            DefinitionOperation::Create(CatalogDefinition::Table(second)),
        ]);

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
        let batch = batch_with(vec![DefinitionOperation::Create(CatalogDefinition::Table(
            table,
        ))]);

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

    #[test]
    fn definition_batch_dry_run_rejects_non_planned_object_version() {
        let mut table = TableDefinition {
            object: object(ObjectKind::Table),
            columns: vec![column("ProductId", 0)],
        };
        table.object.catalog_version = CatalogVersion::new(10);
        let batch = batch_with(vec![DefinitionOperation::Create(CatalogDefinition::Table(
            table,
        ))]);

        let error = batch.dry_run().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("planned next catalog version"));
    }

    #[test]
    fn definition_batch_requires_dependency_friendly_order() {
        let procedure = ProcedureContract {
            object: object_with(2, "Inventory.ReserveStock", ObjectKind::Procedure),
            procedure_id: ProcedureId::new(2),
            contract_hash: ContractHash::test_vector(2),
            inputs: vec![column("ProductId", 0)],
            structured_inputs: vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            result_streams: Vec::new(),
            required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Serializable,
                retryable: false,
            },
            compatibility_policy: CompatibilityPolicy::ExactHash,
        };
        let structured = StructuredObjectDefinition {
            object: object_with(1, "Inventory.StockRequest", ObjectKind::StructuredObject),
            fields: vec![column("ProductId", 0)],
            unique_by: vec!["ProductId".to_string()],
        };
        let batch = batch_with(vec![
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure)),
            DefinitionOperation::Create(CatalogDefinition::StructuredObject(structured)),
        ]);

        let error = batch.dry_run().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("dependencies before dependent"));
    }

    #[test]
    fn catalog_mutation_plan_rejects_sparse_deltas() {
        let table = TableDefinition {
            object: object(ObjectKind::Table),
            columns: vec![column("ProductId", 0)],
        };
        let delta = CatalogMutationDelta::create(
            1,
            CatalogVersion::new(11),
            CatalogDefinition::Table(table),
        );

        let error = CatalogMutationPlan::new(
            DefinitionBatchId::new(4),
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
            CatalogVersion::new(11),
            vec![delta],
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("operation ordered"));
    }
}
