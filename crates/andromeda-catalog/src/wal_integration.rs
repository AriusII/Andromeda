//! Integration between catalog mutations and WAL record emission.
//!
//! This module owns:
//! - Integration point for emitting WAL records when catalog state changes
//! - Conversion from CatalogMutation to CatalogWalRecord
//! - Checkpoint emission with current catalog state
//!
//! ## Design
//!
//! When the catalog applies a mutation (DefinitionBatch apply, procedure alter/drop),
//! this module provides the glue to emit corresponding WAL records. The WAL manager
//! (F1/F4) handles physical durability; this module just assembles the semantic records.
//!
//! No additional file I/O is done here; the WAL manager owns the I/O contract.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::CatalogVersion;

use crate::{CatalogMutation, CatalogWalRecord};

/// Rejects legacy single-record catalog mutation publication.
///
/// # Invariants
///
/// - Catalog DefinitionBatch publication must be represented by the durable
///   `Begin + Apply* + Commit` sequence emitted from `CatalogMutationPlan`.
/// - This helper lacks the ordered operation records and storage-assigned LSN
///   span required to prove all-or-nothing publication.
///
/// # Parameters
///
/// - `mutation`: The catalog version advancement proof
/// - `operator_principal`: User/service identifier that initiated the mutation
///
/// # Note
///
/// This function remains only as a fail-closed compatibility stub. Use
/// `CatalogSystemStore::apply_definition_batch_durably` or
/// `CatalogMutationPlan::records()` for durable catalog publication.
pub fn emit_catalog_mutation_record(
    mutation: &CatalogMutation,
    _operator_principal: String,
) -> AndromedaResult<CatalogWalRecord> {
    if !mutation.is_monotonic() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "catalog mutation WAL emission requires monotonic catalog version advancement",
        ));
    }

    Err(AndromedaError::new(
        AndromedaErrorKind::Catalog,
        "legacy catalog mutation WAL helper cannot prove durable DefinitionBatch publication; use the Begin/Apply/Commit mutation plan sequence",
    ))
}

/// Emits a CatalogCheckpoint record with the current catalog state.
///
/// # Invariants
///
/// - The checkpoint_lsn is the current WAL position.
/// - The catalog_version matches the current visible version.
/// - The visible_procedure_count reflects the current catalog state.
/// - Checkpoints are used as recovery starting points.
///
/// # Parameters
///
/// - `current_lsn`: Current log sequence number (from WAL manager)
/// - `catalog_version`: Current visible catalog version
/// - `visible_procedure_count`: Count of visible procedures at this version
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
    use super::*;

    #[test]
    fn emit_catalog_mutation_record_rejects_legacy_single_record_publication() {
        let mutation = CatalogMutation {
            definition_batch_id: crate::DefinitionBatchId::new(1),
            previous_version: CatalogVersion::new(5),
            next_version: CatalogVersion::new(6),
        };

        let error = emit_catalog_mutation_record(&mutation, "test".to_string()).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(
            error
                .message()
                .contains("durable DefinitionBatch publication")
        );
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
