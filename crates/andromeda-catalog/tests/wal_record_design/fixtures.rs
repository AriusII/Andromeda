use andromeda_catalog_recovery::{AlterCompatibilityPolicy, CatalogWalRecord, DropFailureReason};
use andromeda_definition_batch::DefinitionBatchId;
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};

pub(crate) fn test_hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

pub(crate) fn object_id(value: u64) -> CatalogObjectId {
    CatalogObjectId::new(value)
}

pub(crate) fn create_procedure_record(
    procedure_id: u64,
    name: &str,
    contract_hash: ContractHash,
    dependencies: Vec<CatalogObjectId>,
) -> CatalogWalRecord {
    CatalogWalRecord::CreateProcedure {
        procedure_id: object_id(procedure_id),
        name: name.to_string(),
        contract_hash,
        dependencies,
    }
}

pub(crate) fn alter_procedure_record(
    procedure_id: u64,
    old_contract_hash: ContractHash,
    new_contract_hash: ContractHash,
    compatibility: AlterCompatibilityPolicy,
) -> CatalogWalRecord {
    CatalogWalRecord::AlterProcedure {
        procedure_id: object_id(procedure_id),
        old_contract_hash,
        new_contract_hash,
        compatibility,
    }
}

pub(crate) fn deprecate_procedure_record(procedure_id: u64, reason: &str) -> CatalogWalRecord {
    CatalogWalRecord::DeprecateProcedure {
        procedure_id: object_id(procedure_id),
        reason: reason.to_string(),
    }
}

pub(crate) fn drop_procedure_record(
    procedure_id: u64,
    restrict_failure_reason: Option<DropFailureReason>,
) -> CatalogWalRecord {
    CatalogWalRecord::DropProcedure {
        procedure_id: object_id(procedure_id),
        restrict_failure_reason,
    }
}

pub(crate) fn apply_catalog_version_record(
    batch_id: u64,
    version: u64,
    record_count: usize,
    lsn: u64,
) -> CatalogWalRecord {
    CatalogWalRecord::ApplyCatalogVersion {
        batch_id: DefinitionBatchId::new(batch_id),
        version: CatalogVersion::new(version),
        record_count,
        lsn,
    }
}

pub(crate) fn checkpoint_record(
    checkpoint_lsn: u64,
    catalog_version: u64,
    visible_procedure_count: usize,
) -> CatalogWalRecord {
    CatalogWalRecord::CatalogCheckpoint {
        checkpoint_lsn,
        catalog_version: CatalogVersion::new(catalog_version),
        visible_procedure_count,
    }
}

pub(crate) fn drop_failure_reason(blocker_count: usize, description: &str) -> DropFailureReason {
    DropFailureReason {
        blocker_count,
        description: description.to_string(),
    }
}

pub(crate) fn assert_valid_record(record: &CatalogWalRecord, context: &str) {
    assert!(record.validate().is_ok(), "{context}");
}
