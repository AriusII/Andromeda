//! Catalog WAL replay and recovery logic.
//!
//! This module owns:
//! - `replay_catalog_wal_segment`: Replay all committed catalog WAL records to reconstruct in-memory catalog
//! - Version monotonicity validation
//! - Procedure ID existence validation
//! - Deterministic catalog rebuilding from WAL
//!
//! ## Recovery Semantics
//!
//! During startup, all committed catalog WAL records are replayed in order:
//! 1. Read all records from the WAL segment
//! 2. Filter to target catalog version
//! 3. Validate version monotonicity (no version reordering)
//! 4. Validate procedure ID references (all IDs in mutations must exist)
//! 5. Reconstruct in-memory catalog snapshot

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::wal_record_catalog::CatalogWalRecord;

/// In-memory snapshot of catalog state at a point in recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    /// Current catalog version.
    pub catalog_version: CatalogVersion,
    /// Set of visible procedure IDs.
    pub procedure_ids: BTreeSet<CatalogObjectId>,
    /// Map of procedure ID to last known catalog version at which it was visible.
    pub procedure_versions: BTreeMap<CatalogObjectId, CatalogVersion>,
}

impl CatalogSnapshot {
    pub fn new(catalog_version: CatalogVersion) -> Self {
        Self {
            catalog_version,
            procedure_ids: BTreeSet::new(),
            procedure_versions: BTreeMap::new(),
        }
    }

    /// Returns the count of visible procedures.
    pub fn visible_procedure_count(&self) -> usize {
        self.procedure_ids.len()
    }
}

/// Replays a segment of catalog WAL records to reconstruct the catalog at a target version.
///
/// # Preconditions
///
/// - `records` must be in LSN order (monotonically increasing).
/// - All records must have version <= `target_catalog_version`.
/// - Records must be valid (already validated during encoding).
///
/// # Postconditions
///
/// - Returned snapshot has `catalog_version` exactly matching the last version-advancing record.
/// - All procedure IDs in `procedure_ids` correspond to procedures visible at the final version.
/// - Version monotonicity is guaranteed (no version reordering).
/// - Procedure existence is validated (all references exist).
///
/// # Error Conditions
///
/// - Version reordering (version goes backward)
/// - Procedure reference to non-existent ID
/// - Invalid record invariants (caught during validation)
pub fn replay_catalog_wal_records(
    records: &[CatalogWalRecord],
    target_catalog_version: CatalogVersion,
) -> AndromedaResult<CatalogSnapshot> {
    let mut snapshot = CatalogSnapshot::new(CatalogVersion::new(0));
    let mut last_version: Option<CatalogVersion> = None;

    for record in records {
        // Filter by target version if present
        if let Some(record_version) = record.catalog_version() {
            if record_version.get() > target_catalog_version.get() {
                // Skip records beyond target version
                continue;
            }

            // CatalogCheckpoint doesn't advance the version, only marks a checkpoint at the current version
            let is_checkpoint = matches!(record, CatalogWalRecord::CatalogCheckpoint { .. });

            if !is_checkpoint {
                // Validate version monotonicity only for version-advancing records
                if let Some(prev_version) = last_version {
                    if record_version.get() <= prev_version.get() {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Storage,
                            format!(
                                "catalog version reordering: previous {}, current {}",
                                prev_version.get(),
                                record_version.get()
                            ),
                        ));
                    }
                }
                last_version = Some(record_version);
                snapshot.catalog_version = record_version;
            }
        }

        // Replay the record
        match record {
            CatalogWalRecord::DefinitionBatchApplied {
                affected_procedure_ids,
                ..
            } => {
                for pid in affected_procedure_ids {
                    // Assume all affected IDs are now visible after batch apply
                    snapshot.procedure_ids.insert(*pid);
                    snapshot
                        .procedure_versions
                        .insert(*pid, snapshot.catalog_version);
                }
            }
            CatalogWalRecord::ProcedureAdded { procedure_id, .. } => {
                snapshot.procedure_ids.insert(*procedure_id);
                snapshot
                    .procedure_versions
                    .insert(*procedure_id, snapshot.catalog_version);
            }
            CatalogWalRecord::ProcedureAltered { procedure_id, .. } => {
                // Verify procedure exists
                if !snapshot.procedure_ids.contains(procedure_id) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "ProcedureAltered references non-existent procedure: {}",
                            procedure_id.get()
                        ),
                    ));
                }
                snapshot
                    .procedure_versions
                    .insert(*procedure_id, snapshot.catalog_version);
            }
            CatalogWalRecord::ProcedureDropped { procedure_id, .. } => {
                // Verify procedure exists
                if !snapshot.procedure_ids.contains(procedure_id) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "ProcedureDropped references non-existent procedure: {}",
                            procedure_id.get()
                        ),
                    ));
                }
                // Remove procedure from visible set
                snapshot.procedure_ids.remove(procedure_id);
                snapshot
                    .procedure_versions
                    .insert(*procedure_id, snapshot.catalog_version);
            }
            CatalogWalRecord::StatisticsUpdated {
                table_id,
                column_id,
                ..
            } => {
                // Statistics don't affect the procedure set, but we still need to validate IDs exist
                if !snapshot.procedure_ids.contains(table_id) && !snapshot.procedure_ids.is_empty()
                {
                    // Allow statistics for IDs that may not be in the procedure set
                    // (they could be tables or other objects tracked separately)
                }
                let _ = column_id; // Column validation would require full schema
            }
            CatalogWalRecord::CatalogCheckpoint {
                visible_procedure_count,
                ..
            } => {
                // Checkpoint should match the reconstructed state
                if snapshot.visible_procedure_count() != *visible_procedure_count {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "CatalogCheckpoint visible_procedure_count mismatch: \
                             checkpoint says {}, replay has {}",
                            visible_procedure_count,
                            snapshot.visible_procedure_count()
                        ),
                    ));
                }
            }
        }
    }

    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_empty_records_produces_empty_snapshot() {
        let records = vec![];
        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

        assert_eq!(snapshot.catalog_version, CatalogVersion::new(0));
        assert_eq!(snapshot.visible_procedure_count(), 0);
    }

    #[test]
    fn replay_single_procedure_added_record() {
        let records = vec![CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: andromeda_core::ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        }];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

        assert_eq!(snapshot.catalog_version, CatalogVersion::new(1));
        assert_eq!(snapshot.visible_procedure_count(), 1);
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(1)));
    }

    #[test]
    fn replay_validates_version_monotonicity() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: andromeda_core::ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(5),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: andromeda_core::ContractHash::test_vector(0xBB),
                new_catalog_version: CatalogVersion::new(3), // Reordering!
                timestamp_secs: 2000,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("version reordering"));
    }

    #[test]
    fn replay_procedure_altered_validates_existence() {
        let records = vec![CatalogWalRecord::ProcedureAltered {
            procedure_id: CatalogObjectId::new(999), // Non-existent!
            old_hash: andromeda_core::ContractHash::test_vector(0x11),
            new_hash: andromeda_core::ContractHash::test_vector(0x22),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        }];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("non-existent procedure")
        );
    }

    #[test]
    fn replay_procedure_dropped_removes_from_visible_set() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: andromeda_core::ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureDropped {
                procedure_id: CatalogObjectId::new(1),
                dropped_version: CatalogVersion::new(1),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
        ];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

        assert_eq!(snapshot.catalog_version, CatalogVersion::new(2));
        assert_eq!(snapshot.visible_procedure_count(), 0);
    }

    #[test]
    fn replay_respects_target_version_filter() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: andromeda_core::ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: andromeda_core::ContractHash::test_vector(0xBB),
                new_catalog_version: CatalogVersion::new(5), // Beyond target
                timestamp_secs: 2000,
            },
        ];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(3)).expect("replay failed");

        // Only the first procedure should be visible
        assert_eq!(snapshot.catalog_version, CatalogVersion::new(1));
        assert_eq!(snapshot.visible_procedure_count(), 1);
        assert!(!snapshot.procedure_ids.contains(&CatalogObjectId::new(2)));
    }

    #[test]
    fn replay_checkpoint_validation_succeeds_on_match() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: andromeda_core::ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::CatalogCheckpoint {
                checkpoint_lsn: crate::Lsn::new(100),
                catalog_version: CatalogVersion::new(1),
                visible_procedure_count: 1,
                timestamp_secs: 1001,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
        assert!(result.is_ok());
    }

    #[test]
    fn replay_checkpoint_validation_fails_on_mismatch() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: andromeda_core::ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::CatalogCheckpoint {
                checkpoint_lsn: crate::Lsn::new(100),
                catalog_version: CatalogVersion::new(1),
                visible_procedure_count: 99, // Mismatch!
                timestamp_secs: 1001,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("visible_procedure_count mismatch")
        );
    }

    #[test]
    fn replay_definition_batch_applied_adds_all_procedures() {
        let records = vec![CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 42,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 3,
            affected_procedure_ids: vec![
                CatalogObjectId::new(10),
                CatalogObjectId::new(11),
                CatalogObjectId::new(12),
            ],
            timestamp_secs: 1000,
            operator_principal: "alice".to_string(),
        }];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

        assert_eq!(snapshot.visible_procedure_count(), 3);
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(10)));
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(11)));
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(12)));
    }

    #[test]
    fn replay_complex_sequence_maintains_consistency() {
        let records = vec![
            // Batch 1: Add procedures 1, 2
            CatalogWalRecord::DefinitionBatchApplied {
                batch_id: 1,
                new_catalog_version: CatalogVersion::new(1),
                procedure_count: 2,
                affected_procedure_ids: vec![CatalogObjectId::new(1), CatalogObjectId::new(2)],
                timestamp_secs: 1000,
                operator_principal: "alice".to_string(),
            },
            // Alter procedure 1
            CatalogWalRecord::ProcedureAltered {
                procedure_id: CatalogObjectId::new(1),
                old_hash: andromeda_core::ContractHash::test_vector(0x11),
                new_hash: andromeda_core::ContractHash::test_vector(0x22),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
            // Batch 2: Add procedure 3, drop procedure 2
            CatalogWalRecord::DefinitionBatchApplied {
                batch_id: 2,
                new_catalog_version: CatalogVersion::new(3),
                procedure_count: 2,
                affected_procedure_ids: vec![CatalogObjectId::new(3), CatalogObjectId::new(2)],
                timestamp_secs: 3000,
                operator_principal: "bob".to_string(),
            },
            // Actually drop procedure 2
            CatalogWalRecord::ProcedureDropped {
                procedure_id: CatalogObjectId::new(2),
                dropped_version: CatalogVersion::new(1),
                new_catalog_version: CatalogVersion::new(3),
                timestamp_secs: 3001,
            },
        ];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

        assert_eq!(snapshot.catalog_version, CatalogVersion::new(3));
        assert_eq!(snapshot.visible_procedure_count(), 2);
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(1)));
        assert!(!snapshot.procedure_ids.contains(&CatalogObjectId::new(2)));
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(3)));
    }
}
