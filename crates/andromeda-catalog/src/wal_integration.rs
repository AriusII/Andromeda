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
    CatalogMutationBoundary, CatalogMutationDelta, CatalogMutationOperation, CatalogMutationRecord,
    CatalogPublicationSemantics, CatalogWalRecord,
};

/// Recovery-compatible record types used by durable WAL payload codecs.
pub type RecoveryCatalogMutationRecord = andromeda_catalog_recovery::CatalogMutationRecord;
pub type RecoveryCatalogMutationBoundary = andromeda_catalog_recovery::CatalogMutationBoundary;
pub type RecoveryCatalogMutationDelta = andromeda_catalog_recovery::CatalogMutationDelta;
pub type RecoveryCatalogMutationOperation = andromeda_catalog_recovery::CatalogMutationOperation;

/// Adapts a single catalog mutation boundary to a recovery boundary representation.
pub fn adapt_catalog_mutation_boundary(
    boundary: &CatalogMutationBoundary,
) -> RecoveryCatalogMutationBoundary {
    RecoveryCatalogMutationBoundary {
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
            }
            CatalogPublicationSemantics::DurablePublicationExternal => {
                andromeda_catalog_recovery::CatalogPublicationSemantics::DurablePublicationExternal
            }
        },
    }
}

/// Adapts a local mutation delta to a recovery delta.
pub fn adapt_catalog_mutation_delta(delta: CatalogMutationDelta) -> RecoveryCatalogMutationDelta {
    RecoveryCatalogMutationDelta {
        operation_index: delta.operation_index,
        planned_version: delta.planned_version,
        operation: adapt_catalog_mutation_operation(delta.operation),
    }
}

/// Adapts a local mutation operation to a recovery operation.
pub fn adapt_catalog_mutation_operation(
    operation: CatalogMutationOperation,
) -> RecoveryCatalogMutationOperation {
    match operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            RecoveryCatalogMutationOperation::CreateObject { object, definition }
        }
        CatalogMutationOperation::DeprecateObject { target } => {
            RecoveryCatalogMutationOperation::DeprecateObject {
                target: andromeda_catalog_recovery::CatalogLifecycleTarget {
                    object: target.object,
                },
            }
        }
    }
}

/// Adapts a local mutation record sequence to recovery records.
pub fn adapt_catalog_mutation_records(
    records: impl IntoIterator<Item = CatalogMutationRecord>,
) -> Vec<RecoveryCatalogMutationRecord> {
    records
        .into_iter()
        .map(|record| match record {
            CatalogMutationRecord::Begin(boundary) => {
                RecoveryCatalogMutationRecord::Begin(adapt_catalog_mutation_boundary(&boundary))
            }
            CatalogMutationRecord::Apply(delta) => {
                RecoveryCatalogMutationRecord::Apply(Box::new(adapt_catalog_mutation_delta(*delta)))
            }
            CatalogMutationRecord::Commit(boundary) => {
                RecoveryCatalogMutationRecord::Commit(adapt_catalog_mutation_boundary(&boundary))
            }
        })
        .collect()
}

/// Emits a CatalogCheckpoint record with the current catalog state.
///
/// # Invariants
///
/// - The checkpoint_lsn is the current WAL position.
/// - The catalog_version matches the current visible version.
/// - The visible_procedure_count reflects the current catalog state.
/// - Checkpoints are used as recovery starting points.
pub fn emit_catalog_checkpoint_record(
    current_lsn: u64,
    catalog_version: CatalogVersion,
    visible_procedure_count: usize,
) -> AndromedaResult<CatalogWalRecord> {
    let record = CatalogWalRecord::CatalogCheckpoint {
        checkpoint_lsn: current_lsn,
        catalog_version,
        visible_procedure_count,
    };

    record.validate()?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use crate::CatalogLifecycleTarget;
    use andromeda_catalog_recovery::CatalogPublicationSemantics;
    use andromeda_definition_batch::{
        DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
    };
    use andromeda_error::AndromedaErrorKind;
    use andromeda_procedure_contract::{CatalogObjectId, CatalogObjectRef, ObjectKind};
    use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId, QualifiedName};

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
                }
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
                }
                _ => None,
            })
            .expect("adapted sequence must contain a commit record");

        assert_eq!(commit.previous_version, boundary.previous_version);
        assert_eq!(commit.next_version, boundary.next_version);
    }

    #[test]
    fn emit_catalog_checkpoint_record_produces_valid_record() {
        let record =
            emit_catalog_checkpoint_record(1000, CatalogVersion::new(42), 5).expect("emit failed");

        assert_eq!(record.catalog_version(), Some(CatalogVersion::new(42)));
        assert!(record.validate().is_ok());
    }

    #[test]
    fn emit_catalog_checkpoint_record_rejects_zero_identity() {
        let zero_lsn = emit_catalog_checkpoint_record(0, CatalogVersion::new(42), 5).unwrap_err();
        assert_eq!(zero_lsn.kind(), AndromedaErrorKind::Catalog);
        assert!(zero_lsn.message().contains("checkpoint_lsn"));

        let zero_version =
            emit_catalog_checkpoint_record(1000, CatalogVersion::new(0), 5).unwrap_err();
        assert_eq!(zero_version.kind(), AndromedaErrorKind::Catalog);
        assert!(zero_version.message().contains("catalog_version"));
    }
}
