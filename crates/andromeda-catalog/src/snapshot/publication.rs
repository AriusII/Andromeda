//! Mutation-plan application and durable publication for catalog snapshots.
//!
//! This module contains the write path for [`CatalogSnapshot`]: in-memory
//! apply ([`CatalogSnapshot::apply_mutation_plan`]) and durable publication
//! ([`CatalogSnapshot::publish_durable_mutation_plan`]).  Both delegate to
//! the shared internal method [`CatalogSnapshot::apply_mutation_plan_internal`].

use std::collections::BTreeSet;

use andromeda_catalog_store::{
    CatalogObjectLifecycle, CatalogObjectLifecycleStatus, CatalogSnapshotApplyReport,
};
use andromeda_definition_batch::validate_in_batch_dependencies;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    CatalogMutationCommitEvidence, CatalogMutationOperation, CatalogMutationPlan,
    CatalogPublicationReceipt, DefinitionOperation, PlannedDefinition, PlannedLifecycleTransition,
};

use super::core::CatalogSnapshot;
use super::types::CatalogSnapshotPublication;

impl CatalogSnapshot {
    /// Applies `plan` to this snapshot in memory only.
    ///
    /// The internally applied version advances, but [`Self::visible_version`]
    /// and [`Self::publication`] remain [`CatalogSnapshotPublication::InMemoryOnly`].
    /// No durable evidence is produced; the caller must separately persist the
    /// WAL before treating the resulting state as visible truth.
    pub fn apply_mutation_plan(
        &mut self,
        plan: &CatalogMutationPlan,
    ) -> AndromedaResult<CatalogSnapshotApplyReport> {
        self.apply_mutation_plan_internal(plan, None)
    }

    /// Applies `plan` to this snapshot and records `evidence` as durable
    /// publication proof.
    ///
    /// On success the snapshot's [`Self::publication`] transitions to
    /// [`CatalogSnapshotPublication::Durable`] and [`Self::visible_version`]
    /// advances to `plan.next_version`. The returned
    /// [`CatalogPublicationReceipt`] witnesses exactly this version.
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
                },
                CatalogMutationOperation::DeprecateObject { target } => {
                    planned_deprecated.push(PlannedLifecycleTransition {
                        object_id: target.object.object_id,
                        name: target.object.name.clone(),
                        kind: target.object.kind,
                        action: crate::CatalogLifecycleAction::Deprecate,
                        planned_version: plan.next_version,
                    });
                    planned_operations.push(DefinitionOperation::Deprecate(target.clone()));
                },
            }
        }
        validate_in_batch_dependencies(&planned_operations)?;

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
                },
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
                },
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
                },
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
                },
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
