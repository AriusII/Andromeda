//! Catalog WAL record types for durability and recovery.
//!
//! This module owns:
//! - `CatalogWalRecord`: High-level record types for catalog mutations (DefinitionBatch,
//!   procedure add/alter/drop, statistics updates)
//! - `CatalogWalRecordVersion`: Version enum for schema evolution
//! - Record validation and LSN binding contracts
//!
//! ## Durability Semantics
//!
//! Every catalog mutation produces exactly one WAL record (deterministic causality):
//! - DefinitionBatchApplied: batch of procedures added/altered/dropped
//! - ProcedureAdded: new procedure entry
//! - ProcedureAltered: procedure contract changed (from E2)
//! - ProcedureDropped: procedure removed (from E3)
//! - StatisticsUpdated: histogram/statistics materialized
//! - CatalogCheckpoint: durability boundary with catalog_version snapshot
//!
//! Records are immutable after emission (no rewriting).
//! Catalog version is incremented on every WAL record emission (monotonic).
//! Recovery replays all committed catalog records in order (deterministic rebuild).

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Lsn;

/// Version indicator for CatalogWalRecord schema evolution.
///
/// Future versions of the catalog WAL format will use this enum to distinguish
/// between encoding schemes without requiring separate file types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CatalogWalRecordVersion {
    /// Initial version (V1): deterministic Protobuf encoding with SHA256 checksum
    V1 = 1,
}

impl CatalogWalRecordVersion {
    pub const CURRENT: Self = Self::V1;

    pub fn as_u16(self) -> u16 {
        self as u16
    }

    pub fn try_from_u16(value: u16) -> AndromedaResult<Self> {
        match value {
            1 => Ok(Self::V1),
            v => Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("unsupported catalog WAL record version: {}", v),
            )),
        }
    }
}

/// High-level WAL record types for catalog mutations.
///
/// These records complement the lower-level mutation records and provide semantic
/// information for recovery correlation and operation tracking.
///
/// ## Record Types
///
/// - **DefinitionBatchApplied**: A batch of procedure definitions (add, alter, drop)
/// - **ProcedureAdded**: A new procedure is added to the catalog
/// - **ProcedureAltered**: An existing procedure's contract is replaced
/// - **ProcedureDropped**: An existing procedure is removed from the catalog
/// - **StatisticsUpdated**: Histogram/statistics data materialized
/// - **CatalogCheckpoint**: Durability boundary with version snapshot
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogWalRecord {
    /// A batch of procedure definitions has been applied to the catalog.
    ///
    /// This record represents a [`DefinitionBatch`] that was successfully
    /// planned, applied, and committed to the WAL.
    ///
    /// Invariants:
    /// - `batch_id` must be unique across all batches in the WAL.
    /// - `new_catalog_version` is the visible version after this batch.
    /// - `procedure_count` is `affected_procedure_ids.len()`.
    /// - All procedure IDs in `affected_procedure_ids` are valid in the new catalog.
    /// - `timestamp` is monotonically non-decreasing across consecutive records.
    DefinitionBatchApplied {
        /// Unique batch identifier from the DefinitionBatch.
        batch_id: u64,
        /// The catalog version after batch application.
        new_catalog_version: CatalogVersion,
        /// Number of procedures affected by this batch.
        procedure_count: usize,
        /// IDs of all procedures added, altered, or dropped in this batch.
        affected_procedure_ids: Vec<CatalogObjectId>,
        /// Unix timestamp (seconds since epoch) when batch was applied.
        timestamp_secs: u64,
        /// Operator principal (user/service identifier) that applied the batch.
        operator_principal: String,
    },

    /// A new procedure has been added to the catalog.
    ///
    /// This record indicates a new procedure was created and is now visible
    /// at the new catalog version.
    ///
    /// Invariants:
    /// - `procedure_id` must be unique (never reused).
    /// - `signature_hash` is the canonical contract hash of the procedure.
    /// - `new_catalog_version` is monotonically greater than all prior versions.
    ProcedureAdded {
        /// Unique identifier of the added procedure.
        procedure_id: CatalogObjectId,
        /// Canonical contract hash (sha256) of the procedure signature.
        signature_hash: ContractHash,
        /// The catalog version at which this procedure becomes visible.
        new_catalog_version: CatalogVersion,
        /// Unix timestamp (seconds since epoch).
        timestamp_secs: u64,
    },

    /// An existing procedure's contract has been altered.
    ///
    /// This record indicates a procedure was modified in place, potentially
    /// changing its contract hash (and thus invocation compatibility).
    ///
    /// Invariants (from E2 design):
    /// - `procedure_id` must exist in the prior catalog version.
    /// - `old_hash` is the contract hash before alteration.
    /// - `new_hash` is the contract hash after alteration.
    /// - `new_catalog_version` is monotonically greater than all prior versions.
    /// - Recovery must validate that `procedure_id` exists at prior version.
    ProcedureAltered {
        /// Unique identifier of the altered procedure (unchanged).
        procedure_id: CatalogObjectId,
        /// Previous contract hash (for historical tracking and rollback).
        old_hash: ContractHash,
        /// New contract hash (new binding hash for invocations).
        new_hash: ContractHash,
        /// The catalog version at which this alteration becomes visible.
        new_catalog_version: CatalogVersion,
        /// Unix timestamp (seconds since epoch).
        timestamp_secs: u64,
    },

    /// An existing procedure has been dropped from the catalog.
    ///
    /// This record indicates a procedure was removed and is no longer visible
    /// for new invocations (though historical records may reference it).
    ///
    /// Invariants (from E3 design):
    /// - `procedure_id` must exist in the prior catalog version.
    /// - `dropped_version` is the version at which the procedure was active.
    /// - `new_catalog_version` is the version after removal.
    /// - Recovery must validate that `procedure_id` exists at prior version.
    ProcedureDropped {
        /// Unique identifier of the dropped procedure.
        procedure_id: CatalogObjectId,
        /// The catalog version at which this procedure was active (before drop).
        dropped_version: CatalogVersion,
        /// The catalog version at which the procedure is no longer visible.
        new_catalog_version: CatalogVersion,
        /// Unix timestamp (seconds since epoch).
        timestamp_secs: u64,
    },

    /// Statistics (histogram data) have been materialized and durable.
    ///
    /// This record indicates that histogram statistics for a table column
    /// have been computed and persisted. The histogram data itself is stored
    /// separately; this record just marks the durability boundary.
    ///
    /// Invariants:
    /// - `table_id` and `column_id` reference valid catalog objects.
    /// - `histogram_data_lsn` points to the WAL location where histogram data begins.
    /// - `stats_version` is monotonically increasing per table.
    StatisticsUpdated {
        /// Statistics version (monotonic per table).
        stats_version: u64,
        /// Table identifier that statistics apply to.
        table_id: CatalogObjectId,
        /// Column identifier within the table.
        column_id: CatalogObjectId,
        /// LSN offset where histogram data is stored in the WAL.
        histogram_data_lsn: Lsn,
        /// Unix timestamp (seconds since epoch).
        timestamp_secs: u64,
    },

    /// Catalog durability checkpoint with version snapshot.
    ///
    /// This record marks a durability boundary: all catalog mutations up to
    /// `checkpoint_lsn` have been persisted. Recovery replays records from
    /// this checkpoint forward to restore the catalog.
    ///
    /// Invariants:
    /// - `checkpoint_lsn` is monotonically increasing.
    /// - `catalog_version` corresponds to the visible version at this LSN.
    /// - `visible_procedure_count` matches the catalog snapshot at this version.
    /// - All catalog records before this checkpoint must be durably persisted.
    CatalogCheckpoint {
        /// LSN (log sequence number) of this checkpoint.
        checkpoint_lsn: Lsn,
        /// The catalog version visible at this checkpoint.
        catalog_version: CatalogVersion,
        /// Count of visible procedures in the catalog at this version.
        visible_procedure_count: usize,
        /// Unix timestamp (seconds since epoch).
        timestamp_secs: u64,
    },
}

impl CatalogWalRecord {
    /// Returns the catalog version associated with this record (if any).
    ///
    /// Some records represent version-advancing mutations (DefinitionBatchApplied,
    /// ProcedureAdded, ProcedureAltered, ProcedureDropped, CatalogCheckpoint),
    /// while StatisticsUpdated does not advance the catalog version.
    pub fn catalog_version(&self) -> Option<CatalogVersion> {
        match self {
            Self::DefinitionBatchApplied {
                new_catalog_version,
                ..
            } => Some(*new_catalog_version),
            Self::ProcedureAdded {
                new_catalog_version,
                ..
            } => Some(*new_catalog_version),
            Self::ProcedureAltered {
                new_catalog_version,
                ..
            } => Some(*new_catalog_version),
            Self::ProcedureDropped {
                new_catalog_version,
                ..
            } => Some(*new_catalog_version),
            Self::StatisticsUpdated { .. } => None,
            Self::CatalogCheckpoint {
                catalog_version, ..
            } => Some(*catalog_version),
        }
    }

    /// Returns the timestamp (seconds since epoch) of this record.
    pub fn timestamp_secs(&self) -> u64 {
        match self {
            Self::DefinitionBatchApplied { timestamp_secs, .. } => *timestamp_secs,
            Self::ProcedureAdded { timestamp_secs, .. } => *timestamp_secs,
            Self::ProcedureAltered { timestamp_secs, .. } => *timestamp_secs,
            Self::ProcedureDropped { timestamp_secs, .. } => *timestamp_secs,
            Self::StatisticsUpdated { timestamp_secs, .. } => *timestamp_secs,
            Self::CatalogCheckpoint { timestamp_secs, .. } => *timestamp_secs,
        }
    }

    /// Validates that the record's invariants hold.
    pub fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::DefinitionBatchApplied {
                procedure_count,
                affected_procedure_ids,
                ..
            } => {
                if *procedure_count == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "DefinitionBatchApplied must have at least one affected procedure",
                    ));
                }
                if *procedure_count != affected_procedure_ids.len() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "procedure_count ({}) does not match affected_procedure_ids.len() ({})",
                            procedure_count,
                            affected_procedure_ids.len()
                        ),
                    ));
                }
                Ok(())
            }
            Self::ProcedureAdded { signature_hash, .. } => {
                if signature_hash.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "ProcedureAdded signature_hash must not be zero",
                    ));
                }
                Ok(())
            }
            Self::ProcedureAltered {
                old_hash, new_hash, ..
            } => {
                if old_hash.is_zero() || new_hash.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "ProcedureAltered hashes must not be zero",
                    ));
                }
                Ok(())
            }
            Self::ProcedureDropped {
                dropped_version,
                new_catalog_version,
                ..
            } => {
                if new_catalog_version.get() <= dropped_version.get() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "ProcedureDropped: new_catalog_version must be greater than dropped_version",
                    ));
                }
                Ok(())
            }
            Self::StatisticsUpdated { .. } => Ok(()),
            Self::CatalogCheckpoint {
                visible_procedure_count,
                ..
            } => {
                if *visible_procedure_count == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "CatalogCheckpoint must have at least one visible procedure",
                    ));
                }
                Ok(())
            }
        }
    }

    /// Returns the current system time as seconds since Unix epoch.
    pub fn current_timestamp_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_wal_record_version_current_is_v1() {
        assert_eq!(
            CatalogWalRecordVersion::CURRENT,
            CatalogWalRecordVersion::V1
        );
        assert_eq!(CatalogWalRecordVersion::CURRENT.as_u16(), 1);
    }

    #[test]
    fn catalog_wal_record_version_try_from_u16_accepts_v1() {
        let v = CatalogWalRecordVersion::try_from_u16(1);
        assert!(v.is_ok());
        assert_eq!(v.unwrap(), CatalogWalRecordVersion::V1);
    }

    #[test]
    fn catalog_wal_record_version_try_from_u16_rejects_invalid() {
        let v = CatalogWalRecordVersion::try_from_u16(99);
        assert!(v.is_err());
        assert_eq!(v.unwrap_err().kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn definition_batch_applied_catalog_version_is_visible() {
        let record = CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(10),
            procedure_count: 1,
            affected_procedure_ids: vec![CatalogObjectId::new(1)],
            timestamp_secs: 1000,
            operator_principal: "test".to_string(),
        };
        assert_eq!(record.catalog_version(), Some(CatalogVersion::new(10)));
    }

    #[test]
    fn statistics_updated_has_no_catalog_version() {
        let record = CatalogWalRecord::StatisticsUpdated {
            stats_version: 1,
            table_id: CatalogObjectId::new(1),
            column_id: CatalogObjectId::new(2),
            histogram_data_lsn: Lsn::new(100),
            timestamp_secs: 1000,
        };
        assert_eq!(record.catalog_version(), None);
    }

    #[test]
    fn procedure_added_validates_non_zero_hash() {
        let record = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::zero(),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };
        let result = record.validate();
        assert!(result.is_err());
    }

    #[test]
    fn procedure_added_accepts_valid_hash() {
        let record = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xFF),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };
        assert!(record.validate().is_ok());
    }

    #[test]
    fn definition_batch_applied_validates_procedure_count_match() {
        let record = CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 5, // Mismatch
            affected_procedure_ids: vec![CatalogObjectId::new(1)],
            timestamp_secs: 1000,
            operator_principal: "test".to_string(),
        };
        let result = record.validate();
        assert!(result.is_err());
    }

    #[test]
    fn procedure_dropped_validates_version_ordering() {
        let record = CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(1),
            dropped_version: CatalogVersion::new(10),
            new_catalog_version: CatalogVersion::new(5), // Invalid: should be > dropped_version
            timestamp_secs: 1000,
        };
        let result = record.validate();
        assert!(result.is_err());
    }

    #[test]
    fn procedure_dropped_accepts_valid_version_ordering() {
        let record = CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(1),
            dropped_version: CatalogVersion::new(5),
            new_catalog_version: CatalogVersion::new(10),
            timestamp_secs: 1000,
        };
        assert!(record.validate().is_ok());
    }

    #[test]
    fn timestamp_secs_reflects_record_type() {
        let ts = 12345u64;
        let record = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xFF),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: ts,
        };
        assert_eq!(record.timestamp_secs(), ts);
    }
}
