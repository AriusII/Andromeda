//! Durable catalog mutation record payload codec.
//!
//! This module owns:
//! - `CatalogWalRecord`: High-level record types for catalog mutations
//! - Codec for encoding/decoding records
//! - Durability semantics documentation
//! - LSN correlation with DefinitionBatch

mod codec;
mod constants;
mod design;

pub use constants::{
    CATALOG_CHANGE_APPLY_WAL_KIND_TAG, CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
    CATALOG_CHANGE_COMMIT_WAL_KIND_TAG,
};
pub use design::CatalogWalRecordDesign;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash,
};

use crate::DefinitionBatchId;

/// High-level WAL record types for catalog mutations.
///
/// These records complement the lower-level `CatalogMutationRecord` types and provide
/// semantic information for recovery correlation and operation tracking.
///
/// # Durability Semantics
///
/// All record types follow the WAL-before-visible-commit doctrine:
/// - Records are written to the WAL before mutations become visible.
/// - The WAL is flushed to durable storage before the catalog version advances.
/// - Recovery uses LSN to correlate records and verify batch completeness.
///
/// # Record Types
///
/// - **CreateProcedure**: A new procedure is created with a specific contract hash.
/// - **AlterProcedure**: An existing procedure's contract is replaced (may change hash).
/// - **DeprecateProcedure**: A procedure is marked as deprecated but remains in history.
/// - **DropProcedure**: A procedure is removed from the active catalog.
/// - **ApplyCatalogVersion**: A batch of mutations has been applied; version is now visible.
/// - **CatalogCheckpoint**: A recovery checkpoint records the visible catalog version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogWalRecord {
    /// Creation of a new procedure.
    ///
    /// Invariants:
    /// - `procedure_id` must be unique across all active procedures.
    /// - `contract_hash` must be the canonical hash of the procedure's contract.
    /// - `dependencies` lists all catalog object IDs that this procedure depends on.
    CreateProcedure {
        /// Unique identifier of the created procedure.
        procedure_id: CatalogObjectId,
        /// Qualified name of the procedure.
        name: String,
        /// Canonical hash of the procedure contract (from ContractHash).
        contract_hash: ContractHash,
        /// List of catalog object IDs this procedure depends on.
        dependencies: Vec<CatalogObjectId>,
    },

    /// Modification of an existing procedure's contract (DEC-022).
    ///
    /// Invariants:
    /// - `procedure_id` must exist in the active catalog at the `previous_version`.
    /// - `new_contract_hash` is the canonical hash of the replacement contract.
    /// - `old_contract_hash` is preserved for history and rollback debugging.
    /// - `compatibility` indicates whether the change is `ExactHash` or `AdditiveOnly`.
    AlterProcedure {
        /// Unique identifier of the altered procedure (unchanged).
        procedure_id: CatalogObjectId,
        /// Previous canonical hash (for historical tracking).
        old_contract_hash: ContractHash,
        /// New canonical hash (new binding hash for invocations).
        new_contract_hash: ContractHash,
        /// Compatibility policy that was validated during dry-run.
        compatibility: AlterCompatibilityPolicy,
    },

    /// Deprecation of an existing procedure (marks it as obsolete).
    ///
    /// Invariants:
    /// - `procedure_id` must exist in the active catalog.
    /// - Deprecated procedures remain queryable for historical purposes.
    /// - New invocation bindings may be rejected (depending on policy).
    /// - `reason` provides human-readable context for the deprecation.
    DeprecateProcedure {
        /// Unique identifier of the deprecated procedure.
        procedure_id: CatalogObjectId,
        /// Human-readable reason for deprecation.
        reason: String,
    },

    /// Removal of an existing procedure from the active catalog (DEC-023).
    ///
    /// Invariants:
    /// - `procedure_id` must exist in the active catalog at the `previous_version`.
    /// - If `restrict_failure_reason` is `None`, the drop succeeded.
    /// - If `restrict_failure_reason` is `Some(_)`, the drop was rejected due to dependencies.
    /// - In restrict mode, the procedure remains active if dependencies exist.
    /// - `record_count` is the number of dependent procedures that blocked the drop.
    DropProcedure {
        /// Unique identifier of the procedure being dropped.
        procedure_id: CatalogObjectId,
        /// If `Some`, the drop was rejected; contains the reason and count of blockers.
        restrict_failure_reason: Option<DropFailureReason>,
    },

    /// Commit record indicating that a batch of mutations has been applied.
    ///
    /// This record marks the durability boundary for a catalog version update.
    /// Recovery uses this record to:
    /// 1. Identify complete batches (all mutations + this commit record).
    /// 2. Verify that the batch ID and version match the manifest.
    /// 3. Correlate LSN ranges for consistency checking.
    ///
    /// Invariants:
    /// - `version` must be exactly `previous_version + 1` (no version skips).
    /// - `batch_id` must correspond to a `DefinitionBatch` in the manifest.
    /// - `record_count` must equal the number of operation records written before this.
    /// - `lsn` must be monotonically increasing across batches.
    ApplyCatalogVersion {
        /// The DefinitionBatchId for correlation with batch manifest.
        batch_id: DefinitionBatchId,
        /// The new catalog version that is now visible after this record is flushed.
        version: CatalogVersion,
        /// Count of individual operation records (Create, Alter, Deprecate, Drop).
        record_count: usize,
        /// Log sequence number of this commit record (assigned by storage WAL).
        lsn: u64,
    },

    /// Recovery checkpoint for the visible catalog state.
    CatalogCheckpoint {
        /// Log sequence number covered by this checkpoint.
        checkpoint_lsn: u64,
        /// Catalog version visible at the checkpoint.
        catalog_version: CatalogVersion,
        /// Number of visible procedures at this catalog version.
        visible_procedure_count: usize,
    },
}

impl CatalogWalRecord {
    /// Return the catalog version carried by records that publish one.
    pub fn catalog_version(&self) -> Option<CatalogVersion> {
        match self {
            Self::ApplyCatalogVersion { version, .. } => Some(*version),
            Self::CatalogCheckpoint {
                catalog_version, ..
            } => Some(*catalog_version),
            _ => None,
        }
    }

    /// Validates the record's internal invariants.
    ///
    /// This performs checks that can be done without external state:
    /// - Field ranges and enums are valid.
    /// - IDs are not zero.
    /// - Text fields are not empty (where applicable).
    ///
    /// This does **not** check cross-record consistency or dependency validity;
    /// those are checked during recovery replay.
    pub fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::CreateProcedure {
                procedure_id,
                name,
                contract_hash,
                dependencies,
            } => {
                if procedure_id.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL create procedure record: procedure_id must not be zero",
                    ));
                }
                if name.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL create procedure record: name must not be empty",
                    ));
                }
                if contract_hash.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL create procedure record: contract_hash must not be zero",
                    ));
                }
                // Dependencies vector may be empty (procedure has no dependencies).
                for dep_id in dependencies {
                    if dep_id.get() == 0 {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog WAL create procedure record: dependency IDs must not be zero",
                        ));
                    }
                }
                Ok(())
            }
            Self::AlterProcedure {
                procedure_id,
                old_contract_hash,
                new_contract_hash,
                compatibility: _,
            } => {
                if procedure_id.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL alter procedure record: procedure_id must not be zero",
                    ));
                }
                if old_contract_hash.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL alter procedure record: old_contract_hash must not be zero",
                    ));
                }
                if new_contract_hash.is_zero() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL alter procedure record: new_contract_hash must not be zero",
                    ));
                }
                Ok(())
            }
            Self::DeprecateProcedure {
                procedure_id,
                reason,
            } => {
                if procedure_id.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL deprecate procedure record: procedure_id must not be zero",
                    ));
                }
                if reason.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL deprecate procedure record: reason must not be empty",
                    ));
                }
                Ok(())
            }
            Self::DropProcedure {
                procedure_id,
                restrict_failure_reason,
            } => {
                if procedure_id.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL drop procedure record: procedure_id must not be zero",
                    ));
                }
                if let Some(reason) = restrict_failure_reason {
                    reason.validate()?;
                }
                Ok(())
            }
            Self::ApplyCatalogVersion {
                batch_id,
                version,
                record_count,
                lsn,
            } => {
                if batch_id.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL apply version record: batch_id must not be zero",
                    ));
                }
                if version.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL apply version record: version must not be zero",
                    ));
                }
                if *record_count == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL apply version record: record_count must not be zero",
                    ));
                }
                if *lsn == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL apply version record: lsn must not be zero",
                    ));
                }
                Ok(())
            }
            Self::CatalogCheckpoint {
                checkpoint_lsn,
                catalog_version,
                visible_procedure_count: _,
            } => {
                if *checkpoint_lsn == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL checkpoint record: checkpoint_lsn must not be zero",
                    ));
                }
                if catalog_version.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog WAL checkpoint record: catalog_version must not be zero",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Compatibility policy for an AlterProcedure operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlterCompatibilityPolicy {
    /// The new contract must have an identical canonical hash (ExactHash mode from DEC-022).
    ExactHash,
    /// The new contract may add columns/streams but must not remove or alter existing ones (AdditiveOnly).
    AdditiveOnly,
}

/// Reason for rejection of a DropProcedure in Restrict mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropFailureReason {
    /// Number of procedures that depend on the one being dropped.
    pub blocker_count: usize,
    /// Human-readable description of the blocking dependencies.
    pub description: String,
}

impl DropFailureReason {
    /// Validates the failure reason's internal invariants.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.blocker_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "drop failure reason: blocker_count must not be zero",
            ));
        }
        if self.description.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "drop failure reason: description must not be empty",
            ));
        }
        Ok(())
    }
}
