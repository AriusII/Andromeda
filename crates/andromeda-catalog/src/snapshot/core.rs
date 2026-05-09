//! `CatalogSnapshot` struct definition and read-only accessors.
//!
//! Construction, identity queries, and version-visibility methods live here.
//! Mutation-applying and planning methods are in sibling sub-modules.

use std::collections::BTreeMap;

use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectLifecycle, CatalogObjectLifecycleStatus,
    CatalogSnapshotPublicationGate, QualifiedName,
};
use andromeda_procedure_contract::ProcedureContract;
use andromeda_types::{CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId, ProcedureId};

use crate::CatalogPublicationReceipt;

use super::types::CatalogSnapshotPublication;

/// A consistent, point-in-time view of all catalog object definitions at a
/// specific catalog version.
///
/// Snapshots enable:
/// - Consistent reads across related objects.
/// - Transaction isolation.
/// - Cache invalidation tracking.
/// - Definition rollback support.
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
    pub(super) last_durable_version: CatalogVersion,
    pub(super) objects_by_id: BTreeMap<CatalogObjectId, CatalogDefinition>,
    pub(super) object_names: BTreeMap<QualifiedName, CatalogObjectId>,
    pub(super) object_lifecycle: BTreeMap<CatalogObjectId, CatalogObjectLifecycle>,
}

impl CatalogSnapshot {
    fn publication_gate(&self) -> CatalogSnapshotPublicationGate<CatalogPublicationReceipt> {
        CatalogSnapshotPublicationGate {
            version: self.version,
            publication: self.publication,
            last_durable_version: self.last_durable_version,
        }
    }

    /// Constructs an empty snapshot at the given version with no objects and
    /// no durable publication.
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

    /// Returns the number of objects currently stored in this snapshot.
    pub fn object_count(&self) -> usize {
        self.objects_by_id.len()
    }

    /// Returns the highest catalog version that has been published with
    /// durable evidence. This is the externally visible catalog version
    /// observed by clients; in-memory-only mutations do not advance it.
    pub fn visible_version(&self) -> CatalogVersion {
        self.publication_gate().visible_version()
    }

    /// Returns `true` only when the *currently applied* snapshot state has
    /// durable publication evidence and no further in-memory mutation has
    /// been staged on top of it. This is the canonical predicate downstream
    /// readers must use before treating snapshot contents as externally
    /// publishable truth.
    pub fn is_durably_published(&self) -> bool {
        self.publication_gate().is_durably_published()
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
        self.publication_gate().visible_publication_receipt()
    }

    /// If the snapshot has staged in-memory mutation state ahead of the
    /// visible/durable version, returns that staged version. Returns `None`
    /// when the applied state matches the durable visible version.
    ///
    /// Intended for diagnostics and for readers that explicitly want to
    /// observe the staging gap (never as a substitute for
    /// [`Self::visible_version`]).
    pub fn staged_in_memory_version(&self) -> Option<CatalogVersion> {
        self.publication_gate().staged_in_memory_version()
    }

    /// Recovery-time entry point: mark a catalog version as durably
    /// published because its committed mutation records were observed
    /// during durable WAL replay.
    pub(crate) fn mark_durable_version_from_recovery(&mut self, version: CatalogVersion) {
        if version.get() > self.last_durable_version.get() {
            self.last_durable_version = version;
        }
    }

    /// Returns `true` when an object with `object_id` is present in the
    /// snapshot (regardless of lifecycle status).
    pub fn contains_object_id(&self, object_id: CatalogObjectId) -> bool {
        self.objects_by_id.contains_key(&object_id)
    }

    /// Returns `true` when an object with `name` is present in the snapshot
    /// (regardless of lifecycle status).
    pub fn contains_name(&self, name: &QualifiedName) -> bool {
        self.object_names.contains_key(name)
    }

    /// Looks up a catalog definition by object identifier.
    pub fn get_by_id(&self, object_id: CatalogObjectId) -> Option<&CatalogDefinition> {
        self.objects_by_id.get(&object_id)
    }

    /// Looks up a catalog definition by qualified name.
    pub fn get_by_name(&self, name: &QualifiedName) -> Option<&CatalogDefinition> {
        self.object_names
            .get(name)
            .and_then(|object_id| self.objects_by_id.get(object_id))
    }

    /// Looks up a procedure contract by its procedure identifier.
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

    /// Returns the lifecycle record for `object_id`, or `None` if not found.
    pub fn lifecycle_by_id(&self, object_id: CatalogObjectId) -> Option<CatalogObjectLifecycle> {
        self.object_lifecycle.get(&object_id).copied()
    }

    /// Returns `true` when `object_id` is present and its lifecycle status is
    /// [`Active`](CatalogObjectLifecycleStatus::Active).
    pub fn is_active_object(&self, object_id: CatalogObjectId) -> bool {
        self.lifecycle_by_id(object_id)
            .is_some_and(|lifecycle| lifecycle.status == CatalogObjectLifecycleStatus::Active)
    }
}
