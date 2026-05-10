#![forbid(unsafe_code)]

use andromeda_catalog_store::{CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName};
use andromeda_definition_batch::{
    DefinitionBatch, DefinitionBatchId, DefinitionOperation, SrplBatchDryRunReport,
    dry_run_definition_batch, validate_srpl_operations_dry_run,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, MultiResultPolicy, ProcedureContract,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    StatsVersion, TransactionPolicy,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn object(id: u64, name: &str, version: CatalogVersion) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind: ObjectKind::Procedure,
        catalog_version: version,
    }
}

fn procedure(id: u64, name: &str, version: CatalogVersion) -> ProcedureContract {
    ProcedureContractCandidate {
        object: object(id, name, version),
        procedure_id: ProcedureId::new(id),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs: vec![column("ProductId", 0)],
        structured_inputs: Vec::new(),
        result_streams: Vec::new(),
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: andromeda_procedure_contract::IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: Vec::new(),
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
    .materialize()
    .unwrap()
}

fn batch(contract: ProcedureContract) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(0xD7),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version: CatalogVersion::new(0),
        operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            contract,
        ))],
    }
}

#[test]
fn srpl_batch_dry_run_report_success() {
    let report = SrplBatchDryRunReport::success(2);
    assert!(report.all_valid);
    assert_eq!(report.valid_count, 2);
    assert_eq!(report.rejected_count, 0);
    assert!(report.rejection_reasons.is_empty());
}

#[test]
fn srpl_batch_dry_run_report_failure() {
    let reasons = vec!["syntax error".to_string(), "type mismatch".to_string()];
    let report = SrplBatchDryRunReport::failure(1, reasons.clone());
    assert!(!report.all_valid);
    assert_eq!(report.valid_count, 1);
    assert_eq!(report.rejected_count, 2);
    assert_eq!(report.rejection_reasons, reasons);
}

#[test]
fn srpl_batch_dry_run_counts_materialized_procedure_contracts() {
    let batch = batch(procedure(
        1,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
    ));
    let report = validate_srpl_operations_dry_run(&batch.operations).unwrap();

    assert!(report.all_valid);
    assert_eq!(report.valid_count, 1);
    assert_eq!(report.rejected_count, 0);
}

#[test]
fn srpl_batch_dry_run_rejects_stale_procedure_manifest_hash() {
    let mut contract = procedure(1, "Inventory.ReserveStock", CatalogVersion::new(1));
    contract.stats_version = StatsVersion::new(contract.stats_version.get() + 1);
    let batch = batch(contract);

    let error = dry_run_definition_batch(
        batch.batch_id,
        batch.database_id,
        batch.namespace_id,
        batch.base_version,
        &batch.operations,
    )
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("SRPL dry-run rejected"));
    assert!(error.message().contains("canonical contract shape"));
}
