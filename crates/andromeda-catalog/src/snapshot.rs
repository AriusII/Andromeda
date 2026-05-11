//! Catalog snapshot compatibility facade.
//!
//! Snapshot state, dependency validation, mutation application, and
//! publication gates live in `andromeda-catalog-store`. This module keeps the
//! `andromeda-catalog` API surface required by active reverse dependencies and
//! wires DefinitionBatch planning into the store-owned snapshot.

use std::ops::{Deref, DerefMut};

use andromeda_catalog_store::{
    CatalogSnapshotDefinitionBatchOperation, CatalogSnapshotMutationPlan,
    CatalogSnapshotPlannedObject,
};
use andromeda_definition_batch::{CatalogLifecycleTarget, DefinitionBatch, DefinitionOperation};
use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::{
    CatalogDefinitionBatchPlanning, CatalogMutationCommitEvidence, CatalogMutationDelta,
    CatalogMutationPlan, CatalogPublicationReceipt, DefinitionBatchPlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct CatalogSnapshot(andromeda_catalog_store::CatalogSnapshot<CatalogPublicationReceipt>);

impl CatalogSnapshot {
    pub fn empty(
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        version: CatalogVersion,
    ) -> Self {
        Self(andromeda_catalog_store::CatalogSnapshot::empty(
            database_id,
            namespace_id,
            version,
        ))
    }

    pub fn plan_definition_batch(
        &self,
        batch: &DefinitionBatch,
    ) -> AndromedaResult<DefinitionBatchPlan> {
        let plan = batch.dry_run()?;
        let operations = batch
            .operations
            .iter()
            .map(|operation| match operation {
                DefinitionOperation::Create(definition) => {
                    CatalogSnapshotDefinitionBatchOperation::Create(definition)
                },
                DefinitionOperation::Deprecate(target) => {
                    CatalogSnapshotDefinitionBatchOperation::Deprecate(target)
                },
            })
            .collect::<Vec<_>>();
        let created_objects = plan
            .created_objects
            .iter()
            .map(|created| CatalogSnapshotPlannedObject {
                object_id: created.object_id,
                name: created.name.clone(),
                kind: created.kind,
            })
            .collect::<Vec<_>>();
        let deprecated_objects = plan
            .deprecated_objects
            .iter()
            .map(|deprecated| CatalogSnapshotPlannedObject {
                object_id: deprecated.object_id,
                name: deprecated.name.clone(),
                kind: deprecated.kind,
            })
            .collect::<Vec<_>>();

        self.0.validate_definition_batch_operations(
            batch.database_id,
            batch.namespace_id,
            batch.base_version,
            &operations,
            &created_objects,
            &deprecated_objects,
        )?;

        Ok(plan)
    }

    pub(crate) fn mark_durable_version_from_recovery(&mut self, version: CatalogVersion) {
        self.0.mark_durable_version_from_recovery(version);
    }

    pub fn apply_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
    ) -> AndromedaResult<andromeda_catalog_store::CatalogSnapshotApplyReport> {
        plan.validate_definition_batch_integrity()?;
        self.0.apply_mutation_plan(plan)
    }

    pub fn publish_durable_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
        evidence: CatalogMutationCommitEvidence,
    ) -> AndromedaResult<CatalogPublicationReceipt> {
        plan.validate_definition_batch_integrity()?;
        self.0.publish_durable_mutation_plan(plan, evidence)
    }
}

impl Deref for CatalogSnapshot {
    type Target = andromeda_catalog_store::CatalogSnapshot<CatalogPublicationReceipt>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CatalogSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl CatalogSnapshotMutationPlan for CatalogMutationPlan {
    type LifecycleTarget = CatalogLifecycleTarget;

    fn database_id(&self) -> DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> CatalogVersion {
        self.next_version
    }

    fn publication_semantics(&self) -> andromeda_catalog_store::CatalogPublicationSemantics {
        self.publication_semantics
    }

    fn is_monotonic(&self) -> bool {
        self.mutation().is_monotonic()
    }

    fn deltas(&self) -> &[CatalogMutationDelta] {
        &self.deltas
    }
}
