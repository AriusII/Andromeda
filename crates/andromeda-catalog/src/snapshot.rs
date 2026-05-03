use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    DatabaseId, NamespaceId,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CatalogDefinition, CatalogMutationOperation, CatalogMutationPlan, CatalogPublicationSemantics,
    QualifiedName,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSnapshotPublication {
    InMemoryOnly,
    DurablePublicationExternal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub version: CatalogVersion,
    pub publication: CatalogSnapshotPublication,
    objects_by_id: BTreeMap<CatalogObjectId, CatalogDefinition>,
    object_names: BTreeMap<QualifiedName, CatalogObjectId>,
}

impl CatalogSnapshot {
    pub fn empty(
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        version: CatalogVersion,
    ) -> Self {
        Self {
            database_id,
            namespace_id,
            version,
            publication: CatalogSnapshotPublication::InMemoryOnly,
            objects_by_id: BTreeMap::new(),
            object_names: BTreeMap::new(),
        }
    }

    pub fn object_count(&self) -> usize {
        self.objects_by_id.len()
    }

    pub fn contains_object_id(&self, object_id: CatalogObjectId) -> bool {
        self.objects_by_id.contains_key(&object_id)
    }

    pub fn contains_name(&self, name: &QualifiedName) -> bool {
        self.object_names.contains_key(name)
    }

    pub fn get_by_id(&self, object_id: CatalogObjectId) -> Option<&CatalogDefinition> {
        self.objects_by_id.get(&object_id)
    }

    pub fn apply_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
    ) -> AndromedaResult<CatalogSnapshotApplyReport> {
        if self.database_id != plan.database_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan database id must match snapshot database id",
            ));
        }

        if self.namespace_id != plan.namespace_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan namespace id must match snapshot namespace id",
            ));
        }

        if self.version != plan.previous_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan previous version must match snapshot version",
            ));
        }

        if !plan.mutation().is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan must advance the snapshot version",
            ));
        }

        let mut pending_object_ids = BTreeSet::new();
        let mut pending_object_names = BTreeSet::new();
        for (expected_index, delta) in plan.deltas.iter().enumerate() {
            if delta.operation_index != expected_index {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation deltas must be dense and operation ordered",
                ));
            }

            if delta.planned_version != plan.next_version {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation delta version must match the planned next catalog version",
                ));
            }

            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, definition } => {
                    definition.validate()?;

                    if object.catalog_version != plan.next_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "created object catalog version must match the planned next catalog version",
                        ));
                    }

                    if self.objects_by_id.contains_key(&object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog snapshot already contains object id",
                        ));
                    }

                    if !pending_object_ids.insert(object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not create the same object id twice",
                        ));
                    }

                    if self.object_names.contains_key(&object.name) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog snapshot already contains object name",
                        ));
                    }

                    if !pending_object_names.insert(object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not create the same object name twice",
                        ));
                    }
                }
            }
        }

        for delta in &plan.deltas {
            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, definition } => {
                    self.objects_by_id
                        .insert(object.object_id, definition.clone());
                    self.object_names
                        .insert(object.name.clone(), object.object_id);
                }
            }
        }

        let previous_version = self.version;
        self.version = plan.next_version;

        Ok(CatalogSnapshotApplyReport {
            previous_version,
            next_version: self.version,
            applied_delta_count: plan.deltas.len(),
            publication_semantics: plan.publication_semantics,
            durable_publication_performed: false,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSnapshotApplyReport {
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub applied_delta_count: usize,
    pub publication_semantics: CatalogPublicationSemantics,
    pub durable_publication_performed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CatalogDefinition, CatalogObjectRef, DefinitionBatch, DefinitionBatchId,
        DefinitionOperation, ObjectKind, TableDefinition,
    };
    use andromeda_core::{ColumnDescriptor, ScalarType, TypeDescriptor};

    fn object(id: u64, name: &str, version: CatalogVersion) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind: ObjectKind::Table,
            catalog_version: version,
        }
    }

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn table(id: u64, name: &str, version: CatalogVersion) -> TableDefinition {
        TableDefinition {
            object: object(id, name, version),
            columns: vec![column("ProductId", 0)],
        }
    }

    fn batch(table: TableDefinition, base_version: CatalogVersion) -> DefinitionBatch {
        DefinitionBatch {
            batch_id: DefinitionBatchId::new(7),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version,
            operations: vec![DefinitionOperation::Create(CatalogDefinition::Table(table))],
        }
    }

    #[test]
    fn catalog_snapshot_applies_mutation_plan_in_memory_and_advances_version() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let plan = batch(
            table(1, "Inventory.Product", CatalogVersion::new(11)),
            CatalogVersion::new(10),
        )
            .dry_run()
            .unwrap();

        let report = snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();

        assert_eq!(report.previous_version, CatalogVersion::new(10));
        assert_eq!(report.next_version, CatalogVersion::new(11));
        assert_eq!(report.applied_delta_count, 1);
        assert_eq!(snapshot.version, CatalogVersion::new(11));
        assert_eq!(snapshot.object_count(), 1);
        assert!(snapshot.contains_object_id(CatalogObjectId::new(1)));
        assert!(snapshot.contains_name(&QualifiedName::parse("Inventory.Product").unwrap()));
    }

    #[test]
    fn catalog_snapshot_rejects_existing_duplicate_object_id() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let first = batch(
            table(1, "Inventory.Product", CatalogVersion::new(11)),
            CatalogVersion::new(10),
        )
            .dry_run()
            .unwrap();
        snapshot.apply_mutation_plan(&first.mutation_plan).unwrap();

        let duplicate = batch(
            table(1, "Inventory.Stock", CatalogVersion::new(12)),
            CatalogVersion::new(11),
        )
            .dry_run()
            .unwrap();
        let error = snapshot
            .apply_mutation_plan(&duplicate.mutation_plan)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("object id"));
    }

    #[test]
    fn catalog_snapshot_rejects_stale_plan_version() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(12),
        );
        let plan = batch(
            table(1, "Inventory.Product", CatalogVersion::new(11)),
            CatalogVersion::new(10),
        )
            .dry_run()
            .unwrap();

        let error = snapshot
            .apply_mutation_plan(&plan.mutation_plan)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("previous version"));
    }

    #[test]
    fn catalog_snapshot_apply_does_not_claim_durable_publication_or_storage_coupling() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let plan = batch(
            table(1, "Inventory.Product", CatalogVersion::new(11)),
            CatalogVersion::new(10),
        )
            .dry_run()
            .unwrap();

        let report = snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();

        assert_eq!(
            report.publication_semantics,
            CatalogPublicationSemantics::DurablePublicationExternal
        );
        assert!(!report.durable_publication_performed);
        assert_eq!(
            snapshot.publication,
            CatalogSnapshotPublication::InMemoryOnly
        );
    }
}
