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

use andromeda_core::{AndromedaResult, CatalogVersion};

use crate::{CatalogMutation, CatalogWalRecord};

/// Emits a CatalogWalRecord for a catalog mutation.
///
/// # Invariants
///
/// - Exactly one WAL record is emitted per mutation (deterministic causality).
/// - The record's catalog_version is monotonically greater than prior records.
/// - The record is immutable after emission (no rewriting).
///
/// # Parameters
///
/// - `mutation`: The catalog version advancement proof
/// - `operator_principal`: User/service identifier that initiated the mutation
///
/// # Note
///
/// This function only assembles the record; the caller (WAL manager) is responsible
/// for physical durability.
pub fn emit_catalog_mutation_record(
    mutation: &CatalogMutation,
    _operator_principal: String,
) -> AndromedaResult<CatalogWalRecord> {
    // In a full implementation, record_count and lsn come from the durable
    // DefinitionBatch write pipeline.
    let record = CatalogWalRecord::ApplyCatalogVersion {
        batch_id: mutation.definition_batch_id,
        version: mutation.next_version,
        record_count: 1,
        lsn: 1,
    };

    Ok(record)
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

    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_catalog_mutation_record_produces_valid_record() {
        let mutation = CatalogMutation {
            definition_batch_id: crate::DefinitionBatchId::new(1),
            previous_version: CatalogVersion::new(5),
            next_version: CatalogVersion::new(6),
        };

        let record =
            emit_catalog_mutation_record(&mutation, "test".to_string()).expect("emit failed");

        assert_eq!(record.catalog_version(), Some(CatalogVersion::new(6)));
        assert!(record.validate().is_ok());
    }

    #[test]
    fn emit_catalog_checkpoint_record_produces_valid_record() {
        let record =
            emit_catalog_checkpoint_record(1000, CatalogVersion::new(42), 5).expect("emit failed");

        assert_eq!(record.catalog_version(), Some(CatalogVersion::new(42)));
        assert!(record.validate().is_ok());
    }
}
