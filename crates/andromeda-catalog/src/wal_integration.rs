//! Integration between catalog mutations and WAL record emission.
//!
//! This module owns the explicit adapter boundary for mutation-record emission.
//!
//! The catalog crate keeps live mutation planning and state ownership. Durable
//! payload envelopes and recovery contracts remain explicit in
//! `andromeda-catalog-recovery`. This module converts local catalog mutation
//! records to that boundary when callers need stable replay contracts.

use andromeda_error::AndromedaResult;
use andromeda_types::CatalogVersion;

use crate::{
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationOperation, CatalogMutationPlan,
    CatalogPublicationSemantics, CatalogSnapshot,
};

#[cfg(test)]
use crate::CatalogMutationRecord;

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

/// Adapts a single catalog mutation boundary to a recovery boundary representation.
pub(crate) fn adapt_catalog_mutation_boundary(
    boundary: &CatalogMutationBoundary,
) -> andromeda_catalog_recovery::CatalogMutationBoundary {
    andromeda_catalog_recovery::CatalogMutationBoundary {
        batch_id: boundary.batch_id,
        database_id: boundary.database_id,
        namespace_id: boundary.namespace_id,
        previous_version: boundary.previous_version,
        next_version: boundary.next_version,
        source_hash: boundary.source_hash,
        dependency_graph_hash: andromeda_catalog_recovery::DefinitionBatchDependencyGraphHash::new(
            boundary.dependency_graph_hash.as_bytes(),
        ),
        expected_apply_count: boundary.expected_apply_count,
        publication_semantics: match boundary.publication_semantics {
            CatalogPublicationSemantics::PlannedVersionOnly => {
                andromeda_catalog_recovery::CatalogPublicationSemantics::PlannedVersionOnly
            },
            CatalogPublicationSemantics::DurablePublicationExternal => {
                andromeda_catalog_recovery::CatalogPublicationSemantics::DurablePublicationExternal
            },
        },
    }
}

/// Adapts a local mutation delta to a recovery delta.
pub(crate) fn adapt_catalog_mutation_delta(
    delta: CatalogMutationDelta,
) -> andromeda_catalog_recovery::CatalogMutationDelta {
    andromeda_catalog_recovery::CatalogMutationDelta {
        operation_index: delta.operation_index,
        planned_version: delta.planned_version,
        operation: adapt_catalog_mutation_operation(delta.operation),
    }
}

/// Adapts a local mutation operation to a recovery operation.
pub(crate) fn adapt_catalog_mutation_operation(
    operation: CatalogMutationOperation,
) -> andromeda_catalog_recovery::CatalogMutationOperation {
    match operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            andromeda_catalog_recovery::CatalogMutationOperation::CreateObject {
                object,
                definition,
            }
        },
        CatalogMutationOperation::DeprecateObject { target } => {
            andromeda_catalog_recovery::CatalogMutationOperation::DeprecateObject {
                target: andromeda_catalog_recovery::CatalogLifecycleTarget {
                    object: target.object,
                },
            }
        },
    }
}

#[cfg(test)]
fn adapt_catalog_mutation_records(
    records: impl IntoIterator<Item = CatalogMutationRecord>,
) -> Vec<andromeda_catalog_recovery::CatalogMutationRecord> {
    records
        .into_iter()
        .map(|record| match record {
            CatalogMutationRecord::Begin(boundary) => {
                andromeda_catalog_recovery::CatalogMutationRecord::Begin(
                    adapt_catalog_mutation_boundary(&boundary),
                )
            },
            CatalogMutationRecord::Apply(delta) => {
                andromeda_catalog_recovery::CatalogMutationRecord::Apply(Box::new(
                    adapt_catalog_mutation_delta(*delta),
                ))
            },
            CatalogMutationRecord::Commit(boundary) => {
                andromeda_catalog_recovery::CatalogMutationRecord::Commit(
                    adapt_catalog_mutation_boundary(&boundary),
                )
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::CatalogLifecycleTarget;
    use andromeda_catalog_store::CatalogPublicationSemantics;
    use andromeda_definition_batch::{
        DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
    };
    use andromeda_procedure_contract::{CatalogObjectRef, ObjectKind, QualifiedName};
    use andromeda_types::{CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId};

    use super::*;

    #[test]
    fn adapt_catalog_mutation_records_roundtrips_into_recovery_types() {
        let boundary = CatalogMutationBoundary {
            batch_id: DefinitionBatchId::new(1),
            database_id: DatabaseId::new(10),
            namespace_id: NamespaceId::new(20),
            previous_version: CatalogVersion::new(5),
            next_version: CatalogVersion::new(6),
            source_hash: DefinitionBatchSourceHash::new([0x11; DefinitionBatchSourceHash::LEN]),
            dependency_graph_hash: DefinitionBatchDependencyGraphHash::new([0x22; 32]),
            expected_apply_count: 1,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
        };

        let operation = CatalogMutationOperation::DeprecateObject {
            target: CatalogLifecycleTarget {
                object: CatalogObjectRef {
                    object_id: CatalogObjectId::new(42),
                    name: QualifiedName::parse("Catalog.Proc").unwrap(),
                    kind: ObjectKind::Procedure,
                    catalog_version: CatalogVersion::new(5),
                },
            },
        };

        let local_records = vec![
            CatalogMutationRecord::Begin(boundary),
            CatalogMutationRecord::Apply(Box::new(CatalogMutationDelta {
                operation_index: 0,
                planned_version: CatalogVersion::new(6),
                operation,
            })),
            CatalogMutationRecord::Commit(boundary),
        ];

        let recovered = adapt_catalog_mutation_records(local_records);

        assert_eq!(recovered.len(), 3);
        let begin = recovered
            .iter()
            .find_map(|record| match record {
                andromeda_catalog_recovery::CatalogMutationRecord::Begin(boundary) => {
                    Some(boundary)
                },
                _ => None,
            })
            .expect("adapted sequence must contain a begin record");

        assert_eq!(begin.batch_id, boundary.batch_id);
        assert_eq!(
            begin.dependency_graph_hash.as_bytes(),
            boundary.dependency_graph_hash.as_bytes()
        );
        assert_eq!(
            begin.publication_semantics,
            andromeda_catalog_recovery::CatalogPublicationSemantics::DurablePublicationExternal
        );

        let commit = recovered
            .into_iter()
            .find_map(|record| match record {
                andromeda_catalog_recovery::CatalogMutationRecord::Commit(boundary) => {
                    Some(boundary)
                },
                _ => None,
            })
            .expect("adapted sequence must contain a commit record");

        assert_eq!(commit.previous_version, boundary.previous_version);
        assert_eq!(commit.next_version, boundary.next_version);
    }
}
