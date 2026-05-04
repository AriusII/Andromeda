//! Catalog system store and mutation planning.
//!
//! This module provides the `CatalogSystemStore` facade which validates definition batches
//! against the current catalog snapshot and plans mutations. The store is intentionally
//! stateless regarding durable publication - persistence is delegated to callers.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, DatabaseId, NamespaceId,
};

use crate::{
    CatalogMutationPlan, CatalogSnapshot, CatalogSnapshotApplyReport, DefinitionBatch,
    DefinitionBatchPlan, DefinitionOperation,
};

/// In-memory catalog system facade for definition planning and snapshot mutation.
///
/// This type intentionally owns no durable publication mechanism. It validates that a
/// [`DefinitionBatch`] is planned against the currently visible snapshot and delegates
/// persistence/publication of [`crate::CatalogMutationRecord`] values to callers that
/// own a storage engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSystemStore {
    snapshot: CatalogSnapshot,
}

impl CatalogSystemStore {
    pub fn new(snapshot: CatalogSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn empty(
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        version: CatalogVersion,
    ) -> Self {
        Self::new(CatalogSnapshot::empty(database_id, namespace_id, version))
    }

    pub fn snapshot(&self) -> &CatalogSnapshot {
        &self.snapshot
    }

    pub fn snapshot_mut(&mut self) -> &mut CatalogSnapshot {
        &mut self.snapshot
    }

    pub fn into_snapshot(self) -> CatalogSnapshot {
        self.snapshot
    }

    pub fn plan_definition_batch(
        &self,
        batch: &DefinitionBatch,
    ) -> AndromedaResult<DefinitionBatchPlan> {
        if batch.database_id != self.snapshot.database_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch database id must match catalog snapshot database id",
            ));
        }

        if batch.namespace_id != self.snapshot.namespace_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch namespace id must match catalog snapshot namespace id",
            ));
        }

        if batch.base_version != self.snapshot.version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch base version must match catalog snapshot version",
            ));
        }

        let plan = batch.dry_run()?;

        for operation in &batch.operations {
            let DefinitionOperation::Deprecate(target) = operation else {
                continue;
            };

            let Some(existing) = self.snapshot.get_by_id(target.object.object_id) else {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch cannot deprecate unknown object id",
                ));
            };
            let existing_object = existing.object_ref();
            if existing_object != &target.object {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch lifecycle target must match the current catalog object",
                ));
            }
            if !self.snapshot.is_active_object(target.object.object_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch cannot deprecate an inactive catalog object",
                ));
            }
        }

        Ok(plan)
    }

    pub fn apply_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
    ) -> AndromedaResult<CatalogSnapshotApplyReport> {
        self.snapshot.apply_mutation_plan(plan)
    }

    pub fn apply_definition_batch(
        &mut self,
        batch: &DefinitionBatch,
    ) -> AndromedaResult<CatalogSystemApplyReport> {
        let plan = self.plan_definition_batch(batch)?;
        let snapshot_report = self.apply_mutation_plan(&plan.mutation_plan)?;

        Ok(CatalogSystemApplyReport {
            plan,
            snapshot_report,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSystemApplyReport {
    pub plan: DefinitionBatchPlan,
    pub snapshot_report: CatalogSnapshotApplyReport,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CatalogDefinition, CatalogObjectRef, DefinitionBatchId, DefinitionOperation, ObjectKind,
        QualifiedName, TableDefinition,
    };
    use andromeda_core::{CatalogObjectId, ColumnDescriptor, ScalarType, TypeDescriptor};

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn table(id: u64, name: &str, version: CatalogVersion) -> TableDefinition {
        TableDefinition {
            object: CatalogObjectRef {
                object_id: CatalogObjectId::new(id),
                name: QualifiedName::parse(name).unwrap(),
                kind: ObjectKind::Table,
                catalog_version: version,
            },
            columns: vec![column("ProductId", 0)],
        }
    }

    fn batch(base_version: CatalogVersion) -> DefinitionBatch {
        DefinitionBatch {
            batch_id: DefinitionBatchId::new(1),
            database_id: DatabaseId::new(10),
            namespace_id: NamespaceId::new(20),
            base_version,
            operations: vec![DefinitionOperation::Create(CatalogDefinition::Table(
                table(
                    1,
                    "Inventory.Product",
                    CatalogVersion::new(base_version.get() + 1),
                ),
            ))],
        }
    }

    #[test]
    fn system_store_plans_against_current_snapshot_version() {
        let store = CatalogSystemStore::empty(
            DatabaseId::new(10),
            NamespaceId::new(20),
            CatalogVersion::new(7),
        );

        let error = store
            .plan_definition_batch(&batch(CatalogVersion::new(6)))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("base version"));
    }

    #[test]
    fn system_store_applies_planned_batch_without_durable_publication() {
        let mut store = CatalogSystemStore::empty(
            DatabaseId::new(10),
            NamespaceId::new(20),
            CatalogVersion::new(7),
        );

        let report = store
            .apply_definition_batch(&batch(CatalogVersion::new(7)))
            .unwrap();

        assert_eq!(report.plan.next_version, CatalogVersion::new(8));
        assert_eq!(report.snapshot_report.next_version, CatalogVersion::new(8));
        assert!(!report.snapshot_report.durable_publication_performed);
        assert_eq!(store.snapshot().version, CatalogVersion::new(8));
    }
}
