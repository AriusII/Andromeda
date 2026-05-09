//! Stateful validation of definition batches against a live catalog snapshot.
//!
//! [`CatalogSnapshot::plan_definition_batch`] adds the snapshot-visible checks
//! that cannot be expressed in the purely local [`DefinitionBatch::dry_run`]:
//! stale-base rejection, catalog identity matching, create collisions with
//! existing objects, lifecycle target existence, snapshot-visible catalog
//! dependencies, and active-dependent checks for deprecation.

use std::collections::BTreeSet;

use andromeda_catalog_store::CatalogDefinition;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    CatalogDefinitionBatchPlanning, DefinitionBatch, DefinitionBatchPlan, DefinitionOperation,
    PlannedDefinition, PlannedLifecycleTransition,
};

use super::core::CatalogSnapshot;

impl CatalogSnapshot {
    /// Plan a definition batch against this snapshot's real catalog state.
    ///
    /// [`DefinitionBatch::dry_run`] remains the local/in-batch validator: it
    /// checks operation shape, intra-batch conflicts, planned versions,
    /// dependency order for objects created by the batch, and mutation-record
    /// construction. This method adds the stateful checks that require the
    /// currently visible catalog: stale-base rejection, catalog identity
    /// matching, create collisions with existing objects, lifecycle target
    /// existence, snapshot-visible catalog dependencies, and active dependent
    /// checks for deprecation.
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
                        &definition,
                        &plan.created_objects,
                        &plan.deprecated_objects,
                    )?;
                },
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
                },
            }
        }

        self.validate_no_active_dependents_for_deprecations(&plan.deprecated_objects)?;

        Ok(plan)
    }

    pub(super) fn validate_definition_dependencies(
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

    pub(super) fn validate_no_active_dependents_for_deprecations(
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
}
