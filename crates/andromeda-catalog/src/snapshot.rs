//! Point-in-time catalog snapshots.
//!
//! This module provides catalog snapshots - consistent views of all object definitions
//! at a specific catalog version. Snapshots enable:
//! - Consistent reads across related objects
//! - Transaction isolation
//! - Cache invalidation tracking
//! - Definition rollback support

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    DatabaseId, NamespaceId, ProcedureId,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CatalogDefinition, CatalogMutationCommitEvidence, CatalogMutationOperation,
    CatalogMutationPlan, CatalogPublicationReceipt, CatalogPublicationSemantics, DefinitionBatch,
    DefinitionBatchPlan, DefinitionOperation, PlannedDefinition, PlannedLifecycleTransition,
    ProcedureContract, QualifiedName,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSnapshotPublication {
    InMemoryOnly,
    Durable(CatalogPublicationReceipt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub version: CatalogVersion,
    pub publication: CatalogSnapshotPublication,
    objects_by_id: BTreeMap<CatalogObjectId, CatalogDefinition>,
    object_names: BTreeMap<QualifiedName, CatalogObjectId>,
    object_lifecycle: BTreeMap<CatalogObjectId, CatalogObjectLifecycle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogObjectLifecycleStatus {
    Active,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogObjectLifecycle {
    pub status: CatalogObjectLifecycleStatus,
    pub created_version: CatalogVersion,
    pub last_changed_version: CatalogVersion,
}

impl CatalogObjectLifecycle {
    pub fn active(created_version: CatalogVersion) -> Self {
        Self {
            status: CatalogObjectLifecycleStatus::Active,
            created_version,
            last_changed_version: created_version,
        }
    }

    pub fn deprecated(self, changed_version: CatalogVersion) -> Self {
        Self {
            status: CatalogObjectLifecycleStatus::Deprecated,
            created_version: self.created_version,
            last_changed_version: changed_version,
        }
    }
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
            object_lifecycle: BTreeMap::new(),
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

    pub fn get_by_name(&self, name: &QualifiedName) -> Option<&CatalogDefinition> {
        self.object_names
            .get(name)
            .and_then(|object_id| self.objects_by_id.get(object_id))
    }

    pub fn get_procedure_by_id(&self, procedure_id: ProcedureId) -> Option<&ProcedureContract> {
        self.objects_by_id
            .values()
            .find_map(|definition| match definition {
                CatalogDefinition::Procedure(contract) if contract.procedure_id == procedure_id => {
                    Some(contract)
                }
                _ => None,
            })
    }

    pub fn lifecycle_by_id(&self, object_id: CatalogObjectId) -> Option<CatalogObjectLifecycle> {
        self.object_lifecycle.get(&object_id).copied()
    }

    pub fn is_active_object(&self, object_id: CatalogObjectId) -> bool {
        self.lifecycle_by_id(object_id)
            .is_some_and(|lifecycle| lifecycle.status == CatalogObjectLifecycleStatus::Active)
    }

    /// Plan a definition batch against this snapshot's real catalog state.
    ///
    /// [`DefinitionBatch::dry_run`] remains the local/in-batch validator: it checks
    /// operation shape, intra-batch conflicts, planned versions, dependency order for
    /// objects created by the batch, and mutation-record construction. This snapshot
    /// API adds the stateful checks that require the currently visible catalog:
    /// stale-base rejection, catalog identity matching, create collisions with
    /// existing objects, lifecycle target existence, snapshot-visible catalog
    /// dependencies, and active dependent checks for deprecation.
    pub fn plan_definition_batch(
        &self,
        batch: &DefinitionBatch,
    ) -> AndromedaResult<DefinitionBatchPlan> {
        if batch.database_id != self.database_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch database id must match catalog snapshot database id",
            ));
        }

        if batch.namespace_id != self.namespace_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch namespace id must match catalog snapshot namespace id",
            ));
        }

        if batch.base_version != self.version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch base version must match catalog snapshot version",
            ));
        }

        let plan = batch.dry_run()?;

        for operation in &batch.operations {
            match operation {
                DefinitionOperation::Create(definition) => {
                    let object = definition.object_ref();

                    if self.objects_by_id.contains_key(&object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch cannot create object id already present in catalog snapshot",
                        ));
                    }

                    if self.object_names.contains_key(&object.name) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch cannot create object name already present in catalog snapshot",
                        ));
                    }

                    self.validate_definition_dependencies(
                        definition,
                        &plan.created_objects,
                        &plan.deprecated_objects,
                    )?;
                }
                DefinitionOperation::Deprecate(target) => {
                    let Some(existing) = self.get_by_id(target.object.object_id) else {
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
                    if self.object_names.get(&target.object.name) != Some(&target.object.object_id)
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch lifecycle target name index must match the existing object id",
                        ));
                    }
                    if !self.is_active_object(target.object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch cannot deprecate an inactive catalog object",
                        ));
                    }
                }
            }
        }

        self.validate_no_active_dependents_for_deprecations(&plan.deprecated_objects)?;

        Ok(plan)
    }

    fn validate_definition_dependencies(
        &self,
        definition: &CatalogDefinition,
        created_objects: &[PlannedDefinition],
        deprecated_objects: &[PlannedLifecycleTransition],
    ) -> AndromedaResult<()> {
        for dependency in definition.dependencies() {
            dependency.validate()?;

            if deprecated_objects.iter().any(|target| {
                target.name == dependency.dependency_name
                    && target.kind == dependency.dependency_kind
            }) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch cannot create active dependents of a deprecated catalog dependency",
                ));
            }

            if let Some(created_dependency) = created_objects
                .iter()
                .find(|created| created.name == dependency.dependency_name)
            {
                if created_dependency.kind != dependency.dependency_kind {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "definition batch catalog dependency must reference an object of the expected kind",
                    ));
                }
                continue;
            }

            let Some(existing_dependency) = self.get_by_name(&dependency.dependency_name) else {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch procedure structured input dependency catalog edge is missing from catalog snapshot",
                ));
            };
            let existing_dependency_object = existing_dependency.object_ref();
            if existing_dependency_object.kind != dependency.dependency_kind {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch catalog dependency must reference an object of the expected kind",
                ));
            }
            if !self.is_active_object(existing_dependency_object.object_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "definition batch catalog dependency must reference an active catalog object",
                ));
            }
        }

        Ok(())
    }

    fn validate_no_active_dependents_for_deprecations(
        &self,
        deprecated_objects: &[PlannedLifecycleTransition],
    ) -> AndromedaResult<()> {
        if deprecated_objects.is_empty() {
            return Ok(());
        }

        let deprecated_ids: BTreeSet<_> = deprecated_objects
            .iter()
            .map(|target| target.object_id)
            .collect();

        for definition in self.objects_by_id.values() {
            let dependent_object = definition.object_ref();
            if !self.is_active_object(dependent_object.object_id)
                || deprecated_ids.contains(&dependent_object.object_id)
            {
                continue;
            }

            for dependency in definition.dependencies() {
                if deprecated_objects.iter().any(|target| {
                    target.name == dependency.dependency_name
                        && target.kind == dependency.dependency_kind
                }) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "definition batch cannot deprecate a catalog object with active dependents",
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn apply_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
    ) -> AndromedaResult<CatalogSnapshotApplyReport> {
        self.apply_mutation_plan_internal(plan, None)
    }

    pub fn publish_durable_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
        evidence: CatalogMutationCommitEvidence,
    ) -> AndromedaResult<CatalogPublicationReceipt> {
        let receipt = CatalogPublicationReceipt::from_plan_and_evidence(plan, evidence)?;
        self.apply_mutation_plan_internal(plan, Some(receipt))?;
        Ok(receipt)
    }

    fn apply_mutation_plan_internal(
        &mut self,
        plan: &CatalogMutationPlan,
        durable_publication_receipt: Option<CatalogPublicationReceipt>,
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

        let mut planned_created = Vec::new();
        let mut planned_deprecated = Vec::new();
        let mut planned_operations = Vec::new();
        for delta in &plan.deltas {
            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, definition } => {
                    planned_created.push(PlannedDefinition {
                        object_id: object.object_id,
                        name: object.name.clone(),
                        kind: object.kind,
                        planned_version: plan.next_version,
                    });
                    planned_operations.push(DefinitionOperation::Create(definition.clone()));
                }
                CatalogMutationOperation::DeprecateObject { target } => {
                    planned_deprecated.push(PlannedLifecycleTransition {
                        object_id: target.object.object_id,
                        name: target.object.name.clone(),
                        kind: target.object.kind,
                        action: crate::CatalogLifecycleAction::Deprecate,
                        planned_version: plan.next_version,
                    });
                    planned_operations.push(DefinitionOperation::Deprecate(target.clone()));
                }
            }
        }
        crate::dependencies::validate_in_batch_dependencies(&planned_operations)?;

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
                    self.validate_definition_dependencies(
                        definition,
                        &planned_created,
                        &planned_deprecated,
                    )?;

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
                            "catalog mutation plan must not change the same object id twice",
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
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }
                }
                CatalogMutationOperation::DeprecateObject { target } => {
                    target.validate()?;

                    if target.object.catalog_version > plan.previous_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target version must not be newer than the previous catalog version",
                        ));
                    }

                    if !pending_object_ids.insert(target.object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object id twice",
                        ));
                    }

                    if !pending_object_names.insert(target.object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }

                    let Some(existing) = self.objects_by_id.get(&target.object.object_id) else {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog snapshot cannot deprecate unknown object id",
                        ));
                    };
                    let existing_object = existing.object_ref();
                    if existing_object.name != target.object.name {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target name must match the existing object",
                        ));
                    }
                    if existing_object.kind != target.object.kind {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target kind must match the existing object",
                        ));
                    }
                    if existing_object.catalog_version != target.object.catalog_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target version must match the existing object version",
                        ));
                    }
                    if self.object_names.get(&target.object.name) != Some(&target.object.object_id)
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target name index must match the existing object id",
                        ));
                    }
                    if self
                        .lifecycle_by_id(target.object.object_id)
                        .is_some_and(|lifecycle| {
                            lifecycle.status == CatalogObjectLifecycleStatus::Deprecated
                        })
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog snapshot object is already deprecated",
                        ));
                    }
                }
            }
        }

        self.validate_no_active_dependents_for_deprecations(&planned_deprecated)?;

        for delta in &plan.deltas {
            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, definition } => {
                    self.objects_by_id
                        .insert(object.object_id, definition.clone());
                    self.object_names
                        .insert(object.name.clone(), object.object_id);
                    self.object_lifecycle.insert(
                        object.object_id,
                        CatalogObjectLifecycle::active(object.catalog_version),
                    );
                }
                CatalogMutationOperation::DeprecateObject { target } => {
                    let lifecycle = self
                        .lifecycle_by_id(target.object.object_id)
                        .unwrap_or_else(|| {
                            CatalogObjectLifecycle::active(target.object.catalog_version)
                        });
                    self.object_lifecycle.insert(
                        target.object.object_id,
                        lifecycle.deprecated(plan.next_version),
                    );
                }
            }
        }

        let previous_version = self.version;
        self.version = plan.next_version;
        if let Some(receipt) = durable_publication_receipt {
            self.publication = CatalogSnapshotPublication::Durable(receipt);
        } else {
            self.publication = CatalogSnapshotPublication::InMemoryOnly;
        }

        Ok(CatalogSnapshotApplyReport {
            previous_version,
            next_version: self.version,
            applied_delta_count: plan.deltas.len(),
            publication_semantics: plan.publication_semantics,
            durable_publication_performed: durable_publication_receipt.is_some(),
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
        CatalogDefinition, CatalogLifecycleTarget, CatalogObjectRef, DefinitionBatch,
        DefinitionBatchId, DefinitionOperation, ObjectKind, TableDefinition,
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

    #[test]
    fn catalog_snapshot_applies_deprecation_without_removing_definition() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let create_plan = batch(
            table(1, "Inventory.Product", CatalogVersion::new(11)),
            CatalogVersion::new(10),
        )
        .dry_run()
        .unwrap();
        snapshot
            .apply_mutation_plan(&create_plan.mutation_plan)
            .unwrap();

        let deprecate = DefinitionBatch {
            batch_id: DefinitionBatchId::new(8),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(11),
            operations: vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(1, "Inventory.Product", CatalogVersion::new(11)),
            })],
        }
        .dry_run()
        .unwrap();
        let report = snapshot
            .apply_mutation_plan(&deprecate.mutation_plan)
            .unwrap();

        assert_eq!(report.next_version, CatalogVersion::new(12));
        assert_eq!(snapshot.version, CatalogVersion::new(12));
        assert!(snapshot.contains_object_id(CatalogObjectId::new(1)));
        assert!(snapshot.contains_name(&QualifiedName::parse("Inventory.Product").unwrap()));
        assert!(!snapshot.is_active_object(CatalogObjectId::new(1)));
        assert_eq!(
            snapshot
                .lifecycle_by_id(CatalogObjectId::new(1))
                .unwrap()
                .status,
            CatalogObjectLifecycleStatus::Deprecated
        );
    }

    #[test]
    fn catalog_snapshot_rejects_deprecating_unknown_object() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let deprecate = DefinitionBatch {
            batch_id: DefinitionBatchId::new(8),
            database_id: DatabaseId::new(1),
            namespace_id: NamespaceId::new(2),
            base_version: CatalogVersion::new(10),
            operations: vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(1, "Inventory.Product", CatalogVersion::new(10)),
            })],
        }
        .dry_run()
        .unwrap();

        let error = snapshot
            .apply_mutation_plan(&deprecate.mutation_plan)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("unknown object id"));
    }
}
