//! High-level catalog WAL record contracts and durability semantics.
//!
//! These records are runtime-free recovery contracts. Storage WAL records wrap
//! them, and the live catalog crate adapts them to concrete snapshot state.

use andromeda_definition_batch::DefinitionBatchId;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};

/// Design marker type (unit type, not emitted at runtime).
#[allow(missing_docs)]
pub struct CatalogWalRecordDesign;

/// High-level WAL record types for catalog mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogWalRecord {
    CreateProcedure {
        procedure_id: CatalogObjectId,
        name: String,
        contract_hash: ContractHash,
        dependencies: Vec<CatalogObjectId>,
    },
    AlterProcedure {
        procedure_id: CatalogObjectId,
        old_contract_hash: ContractHash,
        new_contract_hash: ContractHash,
        compatibility: AlterCompatibilityPolicy,
    },
    DeprecateProcedure {
        procedure_id: CatalogObjectId,
        reason: String,
    },
    DropProcedure {
        procedure_id: CatalogObjectId,
        restrict_failure_reason: Option<DropFailureReason>,
    },
    ApplyCatalogVersion {
        batch_id: DefinitionBatchId,
        version: CatalogVersion,
        record_count: usize,
        lsn: u64,
    },
    CatalogCheckpoint {
        checkpoint_lsn: u64,
        catalog_version: CatalogVersion,
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
    pub fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::CreateProcedure {
                procedure_id,
                name,
                contract_hash,
                dependencies,
            } => {
                if procedure_id.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL create procedure record: procedure_id must not be zero",
                    );
                }
                if name.is_empty() {
                    return catalog_recovery_error(
                        "catalog WAL create procedure record: name must not be empty",
                    );
                }
                if contract_hash.is_zero() {
                    return catalog_recovery_error(
                        "catalog WAL create procedure record: contract_hash must not be zero",
                    );
                }
                for dep_id in dependencies {
                    if dep_id.get() == 0 {
                        return catalog_recovery_error(
                            "catalog WAL create procedure record: dependency IDs must not be zero",
                        );
                    }
                }
                Ok(())
            },
            Self::AlterProcedure {
                procedure_id,
                old_contract_hash,
                new_contract_hash,
                compatibility: _,
            } => {
                if procedure_id.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL alter procedure record: procedure_id must not be zero",
                    );
                }
                if old_contract_hash.is_zero() {
                    return catalog_recovery_error(
                        "catalog WAL alter procedure record: old_contract_hash must not be zero",
                    );
                }
                if new_contract_hash.is_zero() {
                    return catalog_recovery_error(
                        "catalog WAL alter procedure record: new_contract_hash must not be zero",
                    );
                }
                Ok(())
            },
            Self::DeprecateProcedure {
                procedure_id,
                reason,
            } => {
                if procedure_id.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL deprecate procedure record: procedure_id must not be zero",
                    );
                }
                if reason.is_empty() {
                    return catalog_recovery_error(
                        "catalog WAL deprecate procedure record: reason must not be empty",
                    );
                }
                Ok(())
            },
            Self::DropProcedure {
                procedure_id,
                restrict_failure_reason,
            } => {
                if procedure_id.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL drop procedure record: procedure_id must not be zero",
                    );
                }
                if let Some(reason) = restrict_failure_reason {
                    reason.validate()?;
                }
                Ok(())
            },
            Self::ApplyCatalogVersion {
                batch_id,
                version,
                record_count,
                lsn,
            } => {
                if batch_id.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL apply version record: batch_id must not be zero",
                    );
                }
                if version.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL apply version record: version must not be zero",
                    );
                }
                if *record_count == 0 {
                    return catalog_recovery_error(
                        "catalog WAL apply version record: record_count must not be zero",
                    );
                }
                if *lsn == 0 {
                    return catalog_recovery_error(
                        "catalog WAL apply version record: lsn must not be zero",
                    );
                }
                Ok(())
            },
            Self::CatalogCheckpoint {
                checkpoint_lsn,
                catalog_version,
                visible_procedure_count: _,
            } => {
                if *checkpoint_lsn == 0 {
                    return catalog_recovery_error(
                        "catalog WAL checkpoint record: checkpoint_lsn must not be zero",
                    );
                }
                if catalog_version.get() == 0 {
                    return catalog_recovery_error(
                        "catalog WAL checkpoint record: catalog_version must not be zero",
                    );
                }
                Ok(())
            },
        }
    }
}

/// Compatibility policy for an AlterProcedure operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlterCompatibilityPolicy {
    ExactHash,
    AdditiveOnly,
}

/// Reason for rejection of a DropProcedure in Restrict mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropFailureReason {
    pub blocker_count: usize,
    pub description: String,
}

impl DropFailureReason {
    /// Validates the failure reason's internal invariants.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.blocker_count == 0 {
            return catalog_recovery_error("drop failure reason: blocker_count must not be zero");
        }
        if self.description.is_empty() {
            return catalog_recovery_error("drop failure reason: description must not be empty");
        }
        Ok(())
    }
}

fn catalog_recovery_error<T>(message: impl Into<String>) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_validates_identity() {
        let record = CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: 10,
            catalog_version: CatalogVersion::new(2),
            visible_procedure_count: 0,
        };

        assert_eq!(record.catalog_version(), Some(CatalogVersion::new(2)));
        assert!(record.validate().is_ok());
    }

    #[test]
    fn drop_failure_reason_rejects_empty_blockers() {
        let reason = DropFailureReason {
            blocker_count: 0,
            description: "blocked".to_string(),
        };

        assert!(reason.validate().is_err());
    }
}
