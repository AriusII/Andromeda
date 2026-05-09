//! Integration between catalog mutations and WAL record emission.
//!
//! The catalog crate keeps live mutation planning and state ownership. Durable
//! payload envelopes, mutation record shapes, and replay contracts are owned by
//! `andromeda-catalog-recovery`; this module only keeps the live
//! `CatalogSnapshot` recovery target implementation and compatibility enum
//! conversions for catalog-local plans.

use andromeda_error::AndromedaResult;
use andromeda_types::CatalogVersion;

use crate::{CatalogMutationPlan, CatalogMutationRecord, CatalogSnapshot};

impl andromeda_catalog_recovery::CatalogRecoveryApplyTarget for CatalogSnapshot {
    fn recovery_database_id(&self) -> andromeda_types::DatabaseId {
        self.database_id
    }

    fn recovery_namespace_id(&self) -> andromeda_types::NamespaceId {
        self.namespace_id
    }

    fn recovery_visible_catalog_version(&self) -> CatalogVersion {
        self.visible_version()
    }

    fn apply_recovered_catalog_mutation(
        &mut self,
        boundary: &andromeda_catalog_recovery::CatalogMutationBoundary,
        deltas: &[andromeda_catalog_recovery::CatalogMutationDelta],
    ) -> AndromedaResult<()> {
        andromeda_catalog_recovery::CatalogRecoveryApplyTarget::validate_recovery_boundary_identity(
            self, boundary,
        )?;

        let mut plan = CatalogMutationPlan::new(
            boundary.batch_id,
            boundary.database_id,
            boundary.namespace_id,
            boundary.previous_version,
            boundary.next_version,
            boundary.source_hash,
            boundary.dependency_graph_hash,
            deltas.to_vec(),
        )?;
        plan.publication_semantics = boundary.publication_semantics;

        self.apply_mutation_plan(&plan)?;
        self.mark_durable_version_from_recovery(boundary.next_version);
        Ok(())
    }
}

impl From<CatalogMutationRecord> for andromeda_catalog_recovery::CatalogMutationRecord {
    fn from(record: CatalogMutationRecord) -> Self {
        match record {
            CatalogMutationRecord::Begin(boundary) => Self::Begin(boundary),
            CatalogMutationRecord::Apply(delta) => Self::Apply(delta),
            CatalogMutationRecord::Commit(boundary) => Self::Commit(boundary),
        }
    }
}

impl From<andromeda_catalog_recovery::CatalogMutationRecord> for CatalogMutationRecord {
    fn from(record: andromeda_catalog_recovery::CatalogMutationRecord) -> Self {
        match record {
            andromeda_catalog_recovery::CatalogMutationRecord::Begin(boundary) => {
                Self::Begin(boundary)
            },
            andromeda_catalog_recovery::CatalogMutationRecord::Apply(delta) => Self::Apply(delta),
            andromeda_catalog_recovery::CatalogMutationRecord::Commit(boundary) => {
                Self::Commit(boundary)
            },
        }
    }
}
