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
    /// Internal applied catalog version. May advance through in-memory apply,
    /// but external callers must treat [`Self::visible_version`] as the
    /// durable, externally observable catalog version.
    ///
    /// **Doctrine:** RAM is never truth. This field reflects staged in-memory
    /// state and may be ahead of [`Self::visible_version`] whenever
    /// [`Self::publication`] is [`CatalogSnapshotPublication::InMemoryOnly`].
    /// Downstream readers building visible answers (planner, executor,
    /// publication observers) must gate on [`Self::is_durably_published`] /
    /// [`Self::visible_publication_receipt`] and never expose this value as
    /// durable truth.
    pub version: CatalogVersion,
    /// Publication tag for the *currently applied* snapshot state.
    ///
    /// `Durable(receipt)` only when the most recent mutation plan was applied
    /// via [`Self::publish_durable_mutation_plan`] (or restored from durable
    /// WAL recovery). Any subsequent in-memory apply resets this to
    /// `InMemoryOnly`, intentionally dropping the prior receipt so it cannot
    /// be misread as evidence for staged state.
    pub publication: CatalogSnapshotPublication,
    /// Highest catalog version for which durable publication evidence has
    /// been observed. Only [`Self::publish_durable_mutation_plan`] (or
    /// recovery from durable WAL evidence) advances this field.
    last_durable_version: CatalogVersion,
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
            last_durable_version: version,
            objects_by_id: BTreeMap::new(),
            object_names: BTreeMap::new(),
            object_lifecycle: BTreeMap::new(),
        }
    }

    pub fn object_count(&self) -> usize {
        self.objects_by_id.len()
    }

    /// Returns the highest catalog version that has been published with
    /// durable evidence. This is the externally visible catalog version
    /// observed by clients; in-memory-only mutations do not advance it.
    pub fn visible_version(&self) -> CatalogVersion {
        self.last_durable_version
    }

    /// Returns `true` only when the *currently applied* snapshot state has
    /// durable publication evidence and no further in-memory mutation has
    /// been staged on top of it. This is the canonical predicate downstream
    /// readers must use before treating snapshot contents as externally
    /// publishable truth.
    pub fn is_durably_published(&self) -> bool {
        matches!(self.publication, CatalogSnapshotPublication::Durable(_))
            && self.version == self.last_durable_version
    }

    /// Returns the publication receipt covering the *currently applied*
    /// state, but only when that state is itself durable. Returns `None`
    /// for `InMemoryOnly` publication and also `None` defensively if a
    /// caller ever mutates the snapshot in a way that desynchronises
    /// `version` from `last_durable_version`.
    ///
    /// Doctrine: a receipt witnesses durable evidence for *exactly* one
    /// catalog version; it must never be returned alongside staged state.
    pub fn visible_publication_receipt(&self) -> Option<CatalogPublicationReceipt> {
        match self.publication {
            CatalogSnapshotPublication::Durable(receipt)
                if self.version == self.last_durable_version
                    && receipt.next_version == self.last_durable_version =>
            {
                Some(receipt)
            }
            _ => None,
        }
    }

    /// If the snapshot has staged in-memory mutation state ahead of the
    /// visible/durable version, returns that staged version. Returns `None`
    /// when the applied state matches the durable visible version.
    ///
    /// Intended for diagnostics and for readers that explicitly want to
    /// observe the staging gap (never as a substitute for
    /// [`Self::visible_version`]).
    pub fn staged_in_memory_version(&self) -> Option<CatalogVersion> {
        if self.version.get() > self.last_durable_version.get() {
            Some(self.version)
        } else {
            None
        }
    }

    /// Recovery-time entry point: mark a catalog version as durably
    /// published because its committed mutation records were observed
    /// during durable WAL replay.
    pub(crate) fn mark_durable_version_from_recovery(&mut self, version: CatalogVersion) {
        if version.get() > self.last_durable_version.get() {
            self.last_durable_version = version;
        }
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
            // Visible/durable catalog version only advances after durable evidence.
            self.last_durable_version = plan.next_version;
        } else {
            self.publication = CatalogSnapshotPublication::InMemoryOnly;
            // last_durable_version intentionally unchanged: in-memory apply
            // must not advance the externally visible catalog version.
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

    // -----------------------------------------------------------------
    // Visible-version (durable evidence) invariants
    // -----------------------------------------------------------------

    #[test]
    fn dry_run_does_not_mutate_snapshot_state_or_visible_version() {
        let snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let snapshot_before = snapshot.clone();

        // Plan against an immutable borrow; this is the catalog dry-run path.
        let plan = snapshot
            .plan_definition_batch(&batch(
                table(1, "Inventory.Product", CatalogVersion::new(11)),
                CatalogVersion::new(10),
            ))
            .unwrap();

        // Plan must be derivable but the snapshot must be byte-for-byte unchanged.
        assert_eq!(plan.next_version, CatalogVersion::new(11));
        assert_eq!(snapshot, snapshot_before);
        assert_eq!(snapshot.version, CatalogVersion::new(10));
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(10));
        assert_eq!(snapshot.object_count(), 0);
    }

    #[test]
    fn in_memory_apply_does_not_advance_visible_catalog_version() {
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

        // Internal applied version advances...
        assert_eq!(snapshot.version, CatalogVersion::new(11));
        assert!(!report.durable_publication_performed);
        // ...but visible/durable version must not move without durable evidence.
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(10));
        assert_eq!(
            snapshot.publication,
            CatalogSnapshotPublication::InMemoryOnly
        );
    }

    #[test]
    fn durable_publication_advances_visible_catalog_version() {
        use crate::{CatalogMutationDurability, CatalogMutationRecord};

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

        // Build a Commit record matching the plan boundary.
        let commit = CatalogMutationRecord::Commit(plan.mutation_plan.commit_boundary());
        let durability = CatalogMutationDurability::StorageWal {
            commit_lsn: 100,
            durable_lsn: 100,
        };
        let evidence = CatalogMutationCommitEvidence::from_durable_commit_record(
            &commit,
            plan.mutation_plan.record_count(),
            durability,
        )
        .unwrap();

        let receipt = snapshot
            .publish_durable_mutation_plan(&plan.mutation_plan, evidence)
            .unwrap();

        assert_eq!(receipt.next_version, CatalogVersion::new(11));
        assert_eq!(snapshot.version, CatalogVersion::new(11));
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(11));
        assert!(matches!(
            snapshot.publication,
            CatalogSnapshotPublication::Durable(_)
        ));
    }

    #[test]
    fn failed_apply_does_not_advance_visible_or_internal_version() {
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(12),
        );
        // Stale plan: previous_version 10 != snapshot.version 12.
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

        // Neither internal nor visible version moved.
        assert_eq!(snapshot.version, CatalogVersion::new(12));
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(12));
        assert_eq!(snapshot.object_count(), 0);
    }

    // -----------------------------------------------------------------
    // Visible-publication accessor guards
    //
    // These regression tests prove that the public accessors used by
    // downstream readers (`is_durably_published`,
    // `visible_publication_receipt`, `staged_in_memory_version`) cannot
    // mistake `InMemoryOnly` staged state for durable visible truth, even
    // when staged work is layered on top of a previously-durable snapshot.
    // -----------------------------------------------------------------

    fn build_evidence(
        plan: &CatalogMutationPlan,
        commit_lsn: u64,
    ) -> CatalogMutationCommitEvidence {
        use crate::{CatalogMutationDurability, CatalogMutationRecord};
        let commit = CatalogMutationRecord::Commit(plan.commit_boundary());
        let durability = CatalogMutationDurability::StorageWal {
            commit_lsn,
            durable_lsn: commit_lsn,
        };
        CatalogMutationCommitEvidence::from_durable_commit_record(
            &commit,
            plan.record_count(),
            durability,
        )
        .unwrap()
    }

    #[test]
    fn empty_snapshot_is_not_durably_published_and_exposes_no_receipt() {
        let snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );

        assert!(!snapshot.is_durably_published());
        assert!(snapshot.visible_publication_receipt().is_none());
        assert!(snapshot.staged_in_memory_version().is_none());
    }

    #[test]
    fn in_memory_apply_exposes_staged_version_but_no_durable_receipt() {
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

        snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();

        // Visible truth must remain at the pre-apply durable version.
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(10));
        assert!(!snapshot.is_durably_published());
        assert!(
            snapshot.visible_publication_receipt().is_none(),
            "InMemoryOnly state must never expose a durable publication receipt"
        );
        assert_eq!(
            snapshot.staged_in_memory_version(),
            Some(CatalogVersion::new(11))
        );
    }

    #[test]
    fn durable_publication_exposes_matching_receipt_and_no_staging_gap() {
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
        let evidence = build_evidence(&plan.mutation_plan, 100);

        let receipt = snapshot
            .publish_durable_mutation_plan(&plan.mutation_plan, evidence)
            .unwrap();

        assert!(snapshot.is_durably_published());
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(11));
        assert_eq!(snapshot.visible_publication_receipt(), Some(receipt));
        assert_eq!(receipt.next_version, snapshot.visible_version());
        assert!(snapshot.staged_in_memory_version().is_none());
    }

    #[test]
    fn in_memory_apply_on_top_of_durable_drops_prior_receipt_and_freezes_visible_version() {
        // This is the doctrinal regression: once staged in-memory work is
        // layered on top of a previously-durable snapshot, the prior
        // durable receipt must NOT be returned, the visible version must
        // freeze at the prior durable version, and `is_durably_published`
        // must report `false` even though `version` advanced.
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
        let evidence = build_evidence(&first.mutation_plan, 100);
        let durable_receipt = snapshot
            .publish_durable_mutation_plan(&first.mutation_plan, evidence)
            .unwrap();
        assert!(snapshot.is_durably_published());
        assert_eq!(
            snapshot.visible_publication_receipt(),
            Some(durable_receipt)
        );

        // Stage a second mutation in-memory only (no durable evidence).
        let staged = batch(
            table(2, "Inventory.Stock", CatalogVersion::new(12)),
            CatalogVersion::new(11),
        )
        .dry_run()
        .unwrap();
        snapshot.apply_mutation_plan(&staged.mutation_plan).unwrap();

        // Internal applied version moves; visible/durable version freezes.
        assert_eq!(snapshot.version, CatalogVersion::new(12));
        assert_eq!(snapshot.visible_version(), CatalogVersion::new(11));
        assert_eq!(
            snapshot.staged_in_memory_version(),
            Some(CatalogVersion::new(12))
        );

        // The prior durable receipt must NOT be exposed: it does not
        // witness the staged state, and surfacing it would let downstream
        // readers misattribute durability.
        assert!(
            !snapshot.is_durably_published(),
            "snapshot with staged work on top of a durable base must not report durable"
        );
        assert!(
            snapshot.visible_publication_receipt().is_none(),
            "prior durable receipt must be dropped once any in-memory mutation stages on top"
        );
        assert_eq!(
            snapshot.publication,
            CatalogSnapshotPublication::InMemoryOnly
        );
    }

    #[test]
    fn recovery_marker_advances_visible_without_publishing_receipt() {
        // Recovery replays durable evidence and may mark a higher durable
        // version, but it does not synthesise a receipt out of thin air;
        // until the snapshot is itself republished durably, the receipt
        // accessor must remain `None` to avoid forging audit evidence.
        let mut snapshot = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        snapshot.mark_durable_version_from_recovery(CatalogVersion::new(15));

        assert_eq!(snapshot.visible_version(), CatalogVersion::new(15));
        assert!(
            snapshot.visible_publication_receipt().is_none(),
            "recovery marker must not fabricate a publication receipt"
        );
    }
}
