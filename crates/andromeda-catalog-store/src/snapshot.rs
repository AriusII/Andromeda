//! Catalog snapshot state and publication gates owned by catalog-store.
//!
//! The catalog engine remains responsible for converting a DefinitionBatch
//! into a mutation plan. This module owns the portable snapshot state,
//! dependency validation, mutation application, and durable-visibility gates.

use std::collections::{BTreeMap, BTreeSet};

use andromeda_contract::CatalogDefinition;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    CatalogObjectRef, ObjectKind, ProcedureContract, QualifiedName,
};
use andromeda_types::CatalogVersion;
use andromeda_types::{CatalogObjectId, DatabaseId, NamespaceId, ProcedureId};

use crate::{
    CatalogLifecycleMutationTarget, CatalogMutationDelta, CatalogMutationOperation,
    CatalogPublicationCommitEvidence, CatalogPublicationPlan, CatalogPublicationSemantics,
};

/// Receipt behavior required by snapshot visibility gates.
pub trait CatalogSnapshotReceipt {
    fn next_version(&self) -> CatalogVersion;
}

impl<DefinitionBatchId, DefinitionBatchSourceHash, DefinitionBatchDependencyGraphHash>
    CatalogSnapshotReceipt
    for crate::CatalogPublicationReceipt<
        DefinitionBatchId,
        DefinitionBatchSourceHash,
        DefinitionBatchDependencyGraphHash,
    >
{
    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }
}

/// Indicates whether the most recently applied mutation plan is backed by
/// durable WAL evidence or is staged in memory only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSnapshotPublication<Receipt> {
    /// The applied state has not been flushed to durable storage.
    InMemoryOnly,
    /// The applied state is covered by a durable publication receipt.
    Durable(Receipt),
}

/// Coarse lifecycle state of a catalog object within a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogObjectLifecycleStatus {
    Active,
    /// The object is retained for history and dependency reasoning but is no
    /// longer active for new catalog resolution.
    Deprecated,
}

/// Lifecycle metadata recorded per catalog object in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogObjectLifecycle {
    pub status: CatalogObjectLifecycleStatus,
    pub created_version: CatalogVersion,
    pub last_changed_version: CatalogVersion,
}

impl CatalogObjectLifecycle {
    /// Constructs an [`Active`](CatalogObjectLifecycleStatus::Active) lifecycle entry at
    /// `created_version`.
    pub fn active(created_version: CatalogVersion) -> Self {
        Self {
            status: CatalogObjectLifecycleStatus::Active,
            created_version,
            last_changed_version: created_version,
        }
    }

    /// Returns a new lifecycle record reflecting domain deprecation at
    /// `changed_version`.
    ///
    /// In this context "deprecated" means "retired from the active catalog",
    /// not "obsolete Rust API".
    pub fn deprecated(self, changed_version: CatalogVersion) -> Self {
        Self {
            status: CatalogObjectLifecycleStatus::Deprecated,
            created_version: self.created_version,
            last_changed_version: changed_version,
        }
    }
}

/// Planned object identity used by snapshot dependency validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshotPlannedObject {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
}

impl CatalogSnapshotPlannedObject {
    pub fn from_object_ref(object: &CatalogObjectRef) -> Self {
        Self {
            object_id: object.object_id,
            name: object.name.clone(),
            kind: object.kind,
        }
    }
}

/// Definition-batch operation view consumed by snapshot validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSnapshotDefinitionBatchOperation<'a, LifecycleTarget> {
    Create(&'a CatalogDefinition),
    Deprecate(&'a LifecycleTarget),
}

/// Describes the outcome of applying a mutation plan to a catalog snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSnapshotApplyReport {
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub applied_delta_count: usize,
    pub publication_semantics: CatalogPublicationSemantics,
    pub durable_publication_performed: bool,
}

/// Minimal mutation-plan view needed by catalog-store snapshots.
pub trait CatalogSnapshotMutationPlan {
    type LifecycleTarget: CatalogLifecycleMutationTarget;

    fn database_id(&self) -> DatabaseId;

    fn namespace_id(&self) -> NamespaceId;

    fn previous_version(&self) -> CatalogVersion;

    fn next_version(&self) -> CatalogVersion;

    fn publication_semantics(&self) -> CatalogPublicationSemantics;

    fn is_monotonic(&self) -> bool;

    fn deltas(&self) -> &[CatalogMutationDelta<Self::LifecycleTarget>];
}

/// Builds a durable publication receipt for a mutation plan and evidence pair.
pub trait CatalogSnapshotReceiptFactory<Plan, Evidence>: Sized {
    fn from_snapshot_publication_plan(plan: &Plan, evidence: Evidence) -> AndromedaResult<Self>;
}

impl<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
    Plan,
    Evidence,
> CatalogSnapshotReceiptFactory<Plan, Evidence>
    for crate::CatalogPublicationReceipt<
        DefinitionBatchId,
        DefinitionBatchSourceHash,
        DefinitionBatchDependencyGraphHash,
    >
where
    DefinitionBatchId: Copy,
    DefinitionBatchSourceHash: Copy,
    DefinitionBatchDependencyGraphHash: Copy,
    Plan: CatalogPublicationPlan<
            DefinitionBatchId,
            DefinitionBatchSourceHash,
            DefinitionBatchDependencyGraphHash,
        >,
    Evidence: CatalogPublicationCommitEvidence<Plan>,
{
    fn from_snapshot_publication_plan(plan: &Plan, evidence: Evidence) -> AndromedaResult<Self> {
        Self::from_plan_and_evidence(plan, evidence)
    }
}

/// Durable-visibility state for the currently applied snapshot version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogSnapshotPublicationGate<Receipt> {
    pub version: CatalogVersion,
    pub publication: CatalogSnapshotPublication<Receipt>,
    pub last_durable_version: CatalogVersion,
}

impl<Receipt> CatalogSnapshotPublicationGate<Receipt>
where
    Receipt: CatalogSnapshotReceipt + Copy,
{
    /// Returns the externally visible catalog version.
    pub fn visible_version(self) -> CatalogVersion {
        self.last_durable_version
    }

    /// Returns `true` only when the currently applied snapshot state has
    /// durable publication evidence and no staged in-memory mutation on top.
    pub fn is_durably_published(self) -> bool {
        matches!(self.publication, CatalogSnapshotPublication::Durable(_))
            && self.version == self.last_durable_version
    }

    /// Returns the durable receipt for the currently applied state.
    pub fn visible_publication_receipt(self) -> Option<Receipt> {
        match self.publication {
            CatalogSnapshotPublication::Durable(receipt)
                if self.version == self.last_durable_version
                    && receipt.next_version() == self.last_durable_version =>
            {
                Some(receipt)
            },
            _ => None,
        }
    }

    /// Returns the staged in-memory version when applied state is ahead of
    /// durable visibility.
    pub fn staged_in_memory_version(self) -> Option<CatalogVersion> {
        if self.version.get() > self.last_durable_version.get() {
            Some(self.version)
        } else {
            None
        }
    }
}

/// A consistent, point-in-time view of catalog object definitions.
///
/// The snapshot owns no WAL or runtime storage. It validates and applies
/// already-planned catalog mutations, and it only advances visible publication
/// when callers provide durable evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot<Receipt> {
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    /// Internal applied catalog version. This may be ahead of
    /// [`Self::visible_version`] after an in-memory-only apply.
    pub version: CatalogVersion,
    pub publication: CatalogSnapshotPublication<Receipt>,
    last_durable_version: CatalogVersion,
    objects_by_id: BTreeMap<CatalogObjectId, CatalogDefinition>,
    object_names: BTreeMap<QualifiedName, CatalogObjectId>,
    object_lifecycle: BTreeMap<CatalogObjectId, CatalogObjectLifecycle>,
}

impl<Receipt> CatalogSnapshot<Receipt>
where
    Receipt: CatalogSnapshotReceipt + Copy,
{
    fn publication_gate(&self) -> CatalogSnapshotPublicationGate<Receipt> {
        CatalogSnapshotPublicationGate {
            version: self.version,
            publication: self.publication,
            last_durable_version: self.last_durable_version,
        }
    }

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

    pub fn visible_version(&self) -> CatalogVersion {
        self.publication_gate().visible_version()
    }

    pub fn is_durably_published(&self) -> bool {
        self.publication_gate().is_durably_published()
    }

    pub fn visible_publication_receipt(&self) -> Option<Receipt> {
        self.publication_gate().visible_publication_receipt()
    }

    pub fn staged_in_memory_version(&self) -> Option<CatalogVersion> {
        self.publication_gate().staged_in_memory_version()
    }

    pub fn mark_durable_version_from_recovery(&mut self, version: CatalogVersion) {
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
                },
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

    pub fn validate_definition_batch_operations<'a, LifecycleTarget>(
        &self,
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        base_version: CatalogVersion,
        operations: &[CatalogSnapshotDefinitionBatchOperation<'a, LifecycleTarget>],
        created_objects: &[CatalogSnapshotPlannedObject],
        deprecated_objects: &[CatalogSnapshotPlannedObject],
    ) -> AndromedaResult<()>
    where
        LifecycleTarget: CatalogLifecycleMutationTarget,
    {
        if database_id != self.database_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch database id must match catalog snapshot database id",
            ));
        }

        if namespace_id != self.namespace_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch namespace id must match catalog snapshot namespace id",
            ));
        }

        if base_version != self.version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch base version must match catalog snapshot version",
            ));
        }

        for operation in operations {
            match operation {
                CatalogSnapshotDefinitionBatchOperation::Create(definition) => {
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
                        created_objects,
                        deprecated_objects,
                    )?;
                },
                CatalogSnapshotDefinitionBatchOperation::Deprecate(target) => {
                    target.validate_for_catalog_mutation()?;
                    let target_object = target.object_ref();
                    let Some(existing) = self.get_by_id(target_object.object_id) else {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch cannot deprecate unknown object id",
                        ));
                    };
                    let existing_object = existing.object_ref();
                    if existing_object != target_object {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch lifecycle target must match the current catalog object",
                        ));
                    }
                    if self.object_names.get(&target_object.name) != Some(&target_object.object_id)
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch lifecycle target name index must match the existing object id",
                        ));
                    }
                    if !self.is_active_object(target_object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "definition batch cannot deprecate an inactive catalog object",
                        ));
                    }
                },
            }
        }

        self.validate_no_active_dependents_for_deprecations(deprecated_objects)
    }

    pub fn validate_definition_dependencies(
        &self,
        definition: &CatalogDefinition,
        created_objects: &[CatalogSnapshotPlannedObject],
        deprecated_objects: &[CatalogSnapshotPlannedObject],
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

    pub fn validate_no_active_dependents_for_deprecations(
        &self,
        deprecated_objects: &[CatalogSnapshotPlannedObject],
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

    pub fn apply_mutation_plan<Plan>(
        &mut self,
        plan: &Plan,
    ) -> AndromedaResult<CatalogSnapshotApplyReport>
    where
        Plan: CatalogSnapshotMutationPlan,
    {
        self.apply_mutation_plan_internal(plan, None)
    }

    pub fn publish_durable_mutation_plan<Plan, Evidence>(
        &mut self,
        plan: &Plan,
        evidence: Evidence,
    ) -> AndromedaResult<Receipt>
    where
        Plan: CatalogSnapshotMutationPlan,
        Receipt: CatalogSnapshotReceiptFactory<Plan, Evidence>,
    {
        let receipt = Receipt::from_snapshot_publication_plan(plan, evidence)?;
        self.apply_mutation_plan_internal(plan, Some(receipt))?;
        Ok(receipt)
    }

    fn apply_mutation_plan_internal<Plan>(
        &mut self,
        plan: &Plan,
        durable_publication_receipt: Option<Receipt>,
    ) -> AndromedaResult<CatalogSnapshotApplyReport>
    where
        Plan: CatalogSnapshotMutationPlan,
    {
        if self.database_id != plan.database_id() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan database id must match snapshot database id",
            ));
        }

        if self.namespace_id != plan.namespace_id() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan namespace id must match snapshot namespace id",
            ));
        }

        if self.version != plan.previous_version() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan previous version must match snapshot version",
            ));
        }

        if !plan.is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan must advance the snapshot version",
            ));
        }

        let mut planned_created = Vec::new();
        let mut planned_deprecated = Vec::new();
        for delta in plan.deltas() {
            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, .. } => {
                    planned_created.push(CatalogSnapshotPlannedObject::from_object_ref(object));
                },
                CatalogMutationOperation::DeprecateObject { target } => {
                    planned_deprecated.push(CatalogSnapshotPlannedObject::from_object_ref(
                        target.object_ref(),
                    ));
                },
            }
        }

        let mut pending_object_ids = BTreeSet::new();
        let mut pending_object_names = BTreeSet::new();
        for (expected_index, delta) in plan.deltas().iter().enumerate() {
            if delta.operation_index != expected_index {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation deltas must be dense and operation ordered",
                ));
            }

            if delta.planned_version != plan.next_version() {
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

                    if object.catalog_version != plan.next_version() {
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
                },
                CatalogMutationOperation::DeprecateObject { target } => {
                    target.validate_for_catalog_mutation()?;
                    let target_object = target.object_ref();

                    if target_object.catalog_version > plan.previous_version() {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target version must not be newer than the previous catalog version",
                        ));
                    }

                    if !pending_object_ids.insert(target_object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object id twice",
                        ));
                    }

                    if !pending_object_names.insert(target_object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }

                    let Some(existing) = self.objects_by_id.get(&target_object.object_id) else {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog snapshot cannot deprecate unknown object id",
                        ));
                    };
                    let existing_object = existing.object_ref();
                    if existing_object.name != target_object.name {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target name must match the existing object",
                        ));
                    }
                    if existing_object.kind != target_object.kind {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target kind must match the existing object",
                        ));
                    }
                    if existing_object.catalog_version != target_object.catalog_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target version must match the existing object version",
                        ));
                    }
                    if self.object_names.get(&target_object.name) != Some(&target_object.object_id)
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog lifecycle target name index must match the existing object id",
                        ));
                    }
                    if self
                        .lifecycle_by_id(target_object.object_id)
                        .is_some_and(|lifecycle| {
                            lifecycle.status == CatalogObjectLifecycleStatus::Deprecated
                        })
                    {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog snapshot object is already deprecated",
                        ));
                    }
                },
            }
        }

        self.validate_no_active_dependents_for_deprecations(&planned_deprecated)?;

        for delta in plan.deltas() {
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
                },
                CatalogMutationOperation::DeprecateObject { target } => {
                    let target_object = target.object_ref();
                    let lifecycle = self
                        .lifecycle_by_id(target_object.object_id)
                        .unwrap_or_else(|| {
                            CatalogObjectLifecycle::active(target_object.catalog_version)
                        });
                    self.object_lifecycle.insert(
                        target_object.object_id,
                        lifecycle.deprecated(plan.next_version()),
                    );
                },
            }
        }

        let previous_version = self.version;
        self.version = plan.next_version();
        if let Some(receipt) = durable_publication_receipt {
            self.publication = CatalogSnapshotPublication::Durable(receipt);
            self.last_durable_version = plan.next_version();
        } else {
            self.publication = CatalogSnapshotPublication::InMemoryOnly;
        }

        Ok(CatalogSnapshotApplyReport {
            previous_version,
            next_version: self.version,
            applied_delta_count: plan.deltas().len(),
            publication_semantics: plan.publication_semantics(),
            durable_publication_performed: durable_publication_receipt.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::StructuredObjectDefinition;
    use andromeda_procedure_contract::{
        AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
        ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
        StatsVersion, TransactionPolicy,
    };
    use andromeda_types::{
        CatalogObjectId, ColumnDescriptor, ContractHash, ProcedureId, ScalarType, TypeDescriptor,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestReceipt {
        next_version: CatalogVersion,
    }

    impl CatalogSnapshotReceipt for TestReceipt {
        fn next_version(&self) -> CatalogVersion {
            self.next_version
        }
    }

    #[derive(Debug)]
    struct TestPlan {
        previous_version: CatalogVersion,
        next_version: CatalogVersion,
        deltas: Vec<CatalogMutationDelta<CatalogObjectRef>>,
    }

    impl CatalogSnapshotMutationPlan for TestPlan {
        type LifecycleTarget = CatalogObjectRef;

        fn database_id(&self) -> DatabaseId {
            DatabaseId::new(1)
        }

        fn namespace_id(&self) -> NamespaceId {
            NamespaceId::new(2)
        }

        fn previous_version(&self) -> CatalogVersion {
            self.previous_version
        }

        fn next_version(&self) -> CatalogVersion {
            self.next_version
        }

        fn publication_semantics(&self) -> CatalogPublicationSemantics {
            CatalogPublicationSemantics::DurablePublicationExternal
        }

        fn is_monotonic(&self) -> bool {
            self.next_version.get() > self.previous_version.get()
        }

        fn deltas(&self) -> &[CatalogMutationDelta<Self::LifecycleTarget>] {
            &self.deltas
        }
    }

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn object(id: u64, name: &str, kind: ObjectKind, version: u64) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: CatalogVersion::new(version),
        }
    }

    fn structured_object(id: u64, name: &str, version: u64) -> CatalogDefinition {
        CatalogDefinition::StructuredObject(StructuredObjectDefinition {
            object: object(id, name, ObjectKind::StructuredObject, version),
            fields: vec![column("ProductId", 0)],
            unique_by: vec!["ProductId".to_string()],
        })
    }

    fn procedure(id: u64, name: &str, version: u64, structured_input: &str) -> CatalogDefinition {
        CatalogDefinition::Procedure(
            ProcedureContractCandidate {
                object: object(id, name, ObjectKind::Procedure, version),
                procedure_id: ProcedureId::new(id),
                stats_version: StatsVersion::new(1),
                protocol_layout: ProtocolLayoutRef {
                    descriptor_set_hash: ContractHash::test_vector(0xA1),
                    frame_envelope_hash: ContractHash::test_vector(0xA2),
                },
                inputs: vec![column("ProductId", 0)],
                structured_inputs: vec![QualifiedName::parse(structured_input).unwrap()],
                result_streams: Vec::new(),
                required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
                transaction_policy: TransactionPolicy {
                    access_mode: AccessMode::ReadWrite,
                    isolation: IsolationPolicy::Serializable,
                    retryable: false,
                },
                compatibility_policy: CompatibilityPolicy::ExactHash,
                result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
                error_policy: ProcedureErrorPolicy {
                    rollback_on_error: true,
                    allowed_error_codes: vec!["InsufficientStock".to_string()],
                },
                multi_result_policy: MultiResultPolicy::SingleResultOnly,
            }
            .materialize()
            .unwrap(),
        )
    }

    fn create_plan(previous_version: u64, definitions: Vec<CatalogDefinition>) -> TestPlan {
        let next_version = CatalogVersion::new(previous_version + 1);
        TestPlan {
            previous_version: CatalogVersion::new(previous_version),
            next_version,
            deltas: definitions
                .into_iter()
                .enumerate()
                .map(|(index, definition)| {
                    CatalogMutationDelta::create(index, next_version, definition)
                })
                .collect(),
        }
    }

    fn deprecate_plan(previous_version: u64, targets: Vec<CatalogObjectRef>) -> TestPlan {
        let next_version = CatalogVersion::new(previous_version + 1);
        TestPlan {
            previous_version: CatalogVersion::new(previous_version),
            next_version,
            deltas: targets
                .into_iter()
                .enumerate()
                .map(|(index, target)| CatalogMutationDelta::deprecate(index, next_version, target))
                .collect(),
        }
    }

    #[test]
    fn durable_publication_gate_exposes_only_current_durable_receipt() {
        let receipt = TestReceipt {
            next_version: CatalogVersion::new(11),
        };
        let gate = CatalogSnapshotPublicationGate {
            version: CatalogVersion::new(11),
            publication: CatalogSnapshotPublication::Durable(receipt),
            last_durable_version: CatalogVersion::new(11),
        };

        assert_eq!(gate.visible_version(), CatalogVersion::new(11));
        assert!(gate.is_durably_published());
        assert_eq!(gate.visible_publication_receipt(), Some(receipt));
        assert_eq!(gate.staged_in_memory_version(), None);
    }

    #[test]
    fn durable_publication_gate_hides_staged_in_memory_state() {
        let receipt = TestReceipt {
            next_version: CatalogVersion::new(11),
        };
        let gate: CatalogSnapshotPublicationGate<TestReceipt> = CatalogSnapshotPublicationGate {
            version: CatalogVersion::new(12),
            publication: CatalogSnapshotPublication::InMemoryOnly,
            last_durable_version: CatalogVersion::new(11),
        };

        assert_eq!(gate.visible_version(), CatalogVersion::new(11));
        assert!(!gate.is_durably_published());
        assert_eq!(gate.visible_publication_receipt(), None);
        assert_eq!(
            gate.staged_in_memory_version(),
            Some(CatalogVersion::new(12))
        );

        let stale_receipt_gate = CatalogSnapshotPublicationGate {
            version: CatalogVersion::new(12),
            publication: CatalogSnapshotPublication::Durable(receipt),
            last_durable_version: CatalogVersion::new(12),
        };

        assert_eq!(stale_receipt_gate.visible_publication_receipt(), None);
    }

    #[test]
    fn snapshot_apply_rejects_missing_catalog_dependency() {
        let mut snapshot: CatalogSnapshot<TestReceipt> = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        let plan = create_plan(
            10,
            vec![procedure(
                2,
                "Inventory.ReserveStock",
                11,
                "Inventory.StockRequest",
            )],
        );

        let error = snapshot.apply_mutation_plan(&plan).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("dependency"));
        assert!(error.message().contains("missing"));
        assert_eq!(snapshot.version, CatalogVersion::new(10));
    }

    #[test]
    fn snapshot_apply_rejects_deprecation_with_active_dependents() {
        let mut snapshot: CatalogSnapshot<TestReceipt> = CatalogSnapshot::empty(
            DatabaseId::new(1),
            NamespaceId::new(2),
            CatalogVersion::new(10),
        );
        snapshot
            .apply_mutation_plan(&create_plan(
                10,
                vec![structured_object(1, "Inventory.StockRequest", 11)],
            ))
            .unwrap();
        snapshot
            .apply_mutation_plan(&create_plan(
                11,
                vec![procedure(
                    2,
                    "Inventory.ReserveStock",
                    12,
                    "Inventory.StockRequest",
                )],
            ))
            .unwrap();

        let error = snapshot
            .apply_mutation_plan(&deprecate_plan(
                12,
                vec![object(
                    1,
                    "Inventory.StockRequest",
                    ObjectKind::StructuredObject,
                    11,
                )],
            ))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("active dependents"));
        assert_eq!(snapshot.version, CatalogVersion::new(12));
    }
}
