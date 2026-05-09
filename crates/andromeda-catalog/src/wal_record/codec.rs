use andromeda_definition_batch::DefinitionBatchDependencyGraphHash;
use andromeda_error::{AndromedaError, AndromedaResult};

use crate::{
    CatalogLifecycleTarget, CatalogMutationBoundary, CatalogMutationDelta,
    CatalogMutationOperation, CatalogMutationRecord, CatalogMutationRecordKind,
    CatalogPublicationSemantics, CatalogWalPayloadDecodeError,
};

type RecoveryBoundary = andromeda_catalog_recovery::CatalogMutationBoundary;
type RecoveryDelta = andromeda_catalog_recovery::CatalogMutationDelta;
type RecoveryRecord = andromeda_catalog_recovery::CatalogMutationRecord;

impl CatalogMutationRecord {
    /// Encode this catalog mutation record as a durable storage-WAL payload.
    ///
    /// The caller must wrap the resulting bytes in a storage WAL record whose
    /// kind tag matches [`CatalogMutationRecordKind::storage_wal_kind_tag`].
    pub fn encode_durable_payload(&self) -> AndromedaResult<Vec<u8>> {
        assert_payload_format_constant_alignment();
        andromeda_catalog_recovery::encode_catalog_durable_payload(&recovery_record_from_catalog(
            self,
        ))
    }

    /// Decode a durable catalog mutation payload.
    ///
    /// This validates the payload header, body checksum, kind/body agreement,
    /// and catalog-local structural invariants. It does not publish the decoded
    /// mutation or replay it into a snapshot.
    pub fn decode_durable_payload(payload: &[u8]) -> AndromedaResult<Self> {
        Self::decode_durable_payload_typed(payload).map_err(AndromedaError::from)
    }

    pub(crate) fn decode_durable_payload_typed(
        payload: &[u8],
    ) -> Result<Self, CatalogWalPayloadDecodeError> {
        assert_payload_format_constant_alignment();
        andromeda_catalog_recovery::decode_catalog_durable_payload(payload)
            .map(catalog_record_from_recovery)
            .map_err(catalog_decode_error)
    }

    pub fn validate_for_durable_payload(&self) -> AndromedaResult<()> {
        assert_payload_format_constant_alignment();
        andromeda_catalog_recovery::validate_catalog_mutation_record(&recovery_record_from_catalog(
            self,
        ))
    }
}

fn recovery_record_from_catalog(record: &CatalogMutationRecord) -> RecoveryRecord {
    match record {
        CatalogMutationRecord::Begin(boundary) => {
            RecoveryRecord::Begin(recovery_boundary_from_catalog(boundary))
        },
        CatalogMutationRecord::Apply(delta) => {
            RecoveryRecord::Apply(Box::new(recovery_delta_from_catalog(delta)))
        },
        CatalogMutationRecord::Commit(boundary) => {
            RecoveryRecord::Commit(recovery_boundary_from_catalog(boundary))
        },
    }
}

fn recovery_boundary_from_catalog(boundary: &CatalogMutationBoundary) -> RecoveryBoundary {
    RecoveryBoundary {
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
        publication_semantics: recovery_publication_semantics(boundary.publication_semantics),
    }
}

fn recovery_delta_from_catalog(delta: &CatalogMutationDelta) -> RecoveryDelta {
    RecoveryDelta {
        operation_index: delta.operation_index,
        planned_version: delta.planned_version,
        operation: recovery_operation_from_catalog(&delta.operation),
    }
}

fn recovery_operation_from_catalog(
    operation: &CatalogMutationOperation,
) -> andromeda_catalog_recovery::CatalogMutationOperation {
    match operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            andromeda_catalog_recovery::CatalogMutationOperation::CreateObject {
                object: object.clone(),
                definition: definition.clone(),
            }
        },
        CatalogMutationOperation::DeprecateObject { target } => {
            andromeda_catalog_recovery::CatalogMutationOperation::DeprecateObject {
                target: andromeda_catalog_recovery::CatalogLifecycleTarget {
                    object: target.object.clone(),
                },
            }
        },
    }
}

fn recovery_publication_semantics(
    semantics: CatalogPublicationSemantics,
) -> andromeda_catalog_recovery::CatalogPublicationSemantics {
    match semantics {
        CatalogPublicationSemantics::PlannedVersionOnly => {
            andromeda_catalog_recovery::CatalogPublicationSemantics::PlannedVersionOnly
        },
        CatalogPublicationSemantics::DurablePublicationExternal => {
            andromeda_catalog_recovery::CatalogPublicationSemantics::DurablePublicationExternal
        },
    }
}

fn catalog_record_from_recovery(record: RecoveryRecord) -> CatalogMutationRecord {
    match record {
        RecoveryRecord::Begin(boundary) => {
            CatalogMutationRecord::Begin(catalog_boundary_from_recovery(boundary))
        },
        RecoveryRecord::Apply(delta) => {
            CatalogMutationRecord::Apply(Box::new(catalog_delta_from_recovery(*delta)))
        },
        RecoveryRecord::Commit(boundary) => {
            CatalogMutationRecord::Commit(catalog_boundary_from_recovery(boundary))
        },
    }
}

fn catalog_boundary_from_recovery(boundary: RecoveryBoundary) -> CatalogMutationBoundary {
    CatalogMutationBoundary {
        batch_id: boundary.batch_id,
        database_id: boundary.database_id,
        namespace_id: boundary.namespace_id,
        previous_version: boundary.previous_version,
        next_version: boundary.next_version,
        source_hash: boundary.source_hash,
        dependency_graph_hash: DefinitionBatchDependencyGraphHash::new(
            boundary.dependency_graph_hash.as_bytes(),
        ),
        expected_apply_count: boundary.expected_apply_count,
        publication_semantics: catalog_publication_semantics(boundary.publication_semantics),
    }
}

fn catalog_delta_from_recovery(delta: RecoveryDelta) -> CatalogMutationDelta {
    CatalogMutationDelta {
        operation_index: delta.operation_index,
        planned_version: delta.planned_version,
        operation: catalog_operation_from_recovery(delta.operation),
    }
}

fn catalog_operation_from_recovery(
    operation: andromeda_catalog_recovery::CatalogMutationOperation,
) -> CatalogMutationOperation {
    match operation {
        andromeda_catalog_recovery::CatalogMutationOperation::CreateObject {
            object,
            definition,
        } => CatalogMutationOperation::CreateObject { object, definition },
        andromeda_catalog_recovery::CatalogMutationOperation::DeprecateObject { target } => {
            CatalogMutationOperation::DeprecateObject {
                target: CatalogLifecycleTarget {
                    object: target.object,
                },
            }
        },
    }
}

fn catalog_publication_semantics(
    semantics: andromeda_catalog_recovery::CatalogPublicationSemantics,
) -> CatalogPublicationSemantics {
    match semantics {
        andromeda_catalog_recovery::CatalogPublicationSemantics::PlannedVersionOnly => {
            CatalogPublicationSemantics::PlannedVersionOnly
        },
        andromeda_catalog_recovery::CatalogPublicationSemantics::DurablePublicationExternal => {
            CatalogPublicationSemantics::DurablePublicationExternal
        },
    }
}

fn catalog_decode_error(
    error: andromeda_catalog_recovery::CatalogWalPayloadDecodeError,
) -> CatalogWalPayloadDecodeError {
    error
}

fn assert_payload_format_constant_alignment() {
    debug_assert_eq!(
        CatalogMutationRecordKind::from_storage_wal_kind_tag(
            andromeda_catalog_recovery::CATALOG_CHANGE_BEGIN_WAL_KIND_TAG
        ),
        Some(CatalogMutationRecordKind::CatalogChangeBegin)
    );
    debug_assert_eq!(
        CatalogMutationRecordKind::from_storage_wal_kind_tag(
            andromeda_catalog_recovery::CATALOG_CHANGE_APPLY_WAL_KIND_TAG
        ),
        Some(CatalogMutationRecordKind::CatalogChangeApply)
    );
    debug_assert_eq!(
        CatalogMutationRecordKind::from_storage_wal_kind_tag(
            andromeda_catalog_recovery::CATALOG_CHANGE_COMMIT_WAL_KIND_TAG
        ),
        Some(CatalogMutationRecordKind::CatalogChangeCommit)
    );
}
