#![forbid(unsafe_code)]

use andromeda_catalog_store::CatalogDefinition;
use andromeda_definition_batch::{DefinitionBatchId, DefinitionOperation};
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, ProcedureErrorPolicy,
    ProtocolLayoutRef, ResultMetadataPolicy, StatsVersion, TransactionPolicy,
};
use andromeda_srpl_definition_batch::{
    SrplDefinitionBatchDryRunRequest, SrplDefinitionBatchProcedureSource,
    compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
    dry_run_srpl_definition_batch_sources,
};
use andromeda_srpl_diagnostics::DiagnosticPhase;
use andromeda_srpl_ir::{Cardinality, SrplProcedureContractMetadata};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, NamespaceId, ProcedureId,
};

const DB_ID: DatabaseId = DatabaseId::new(31);
const NS_ID: NamespaceId = NamespaceId::new(41);

#[test]
fn owner_direct_compiler_pipeline_materializes_ir_without_srpl_facade() {
    let ir = compile_narrow_procedure_signature(signature_source())
        .expect("definition-batch owner compiler must parse, bind, and lower source");

    assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ir.inputs.len(), 1);
    assert_eq!(ir.inputs[0].name, "ProductId");
    assert_eq!(ir.result_streams[0].name, "Reservation");
    assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
    assert!(ir.body.operations.is_empty());
}

#[test]
fn owner_direct_definition_batch_compile_creates_procedure_operation() {
    let batch = compile_narrow_procedure_definition_batch(
        signature_source(),
        metadata(101, 201, CatalogVersion::new(1)),
        DefinitionBatchId::new(301),
        DB_ID,
        NS_ID,
        CatalogVersion::new(0),
    )
    .expect("definition-batch owner must compile source into a DefinitionBatch");

    assert_eq!(batch.operations.len(), 1);
    let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) = &batch.operations[0]
    else {
        panic!("compiled SRPL source must create a Procedure definition");
    };
    assert_eq!(
        contract.object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(contract.procedure_id, ProcedureId::new(201));
    assert!(contract.validate_canonical_hash().is_ok());
}

#[test]
fn owner_direct_dry_run_binds_source_evidence_to_manifest_hashes() {
    let report = dry_run_srpl_definition_batch_sources(SrplDefinitionBatchDryRunRequest {
        batch_id: DefinitionBatchId::new(302),
        database_id: DB_ID,
        namespace_id: NS_ID,
        base_version: CatalogVersion::new(0),
        procedures: vec![SrplDefinitionBatchProcedureSource::new(
            signature_source(),
            metadata(102, 202, CatalogVersion::new(1)),
        )],
    })
    .expect("valid SRPL source should dry-run through the owner crate");

    assert_eq!(report.definition_batch.operations.len(), 1);
    assert_eq!(report.plan.operation_count, 1);
    assert_eq!(report.manifests.len(), 1);
    assert_eq!(
        report.definition_batch_source_hash,
        report.definition_batch.source_hash()
    );
    assert_eq!(
        report.source_evidence.definition_batch_source_hash,
        report.definition_batch_source_hash
    );
    assert_eq!(
        report.source_evidence.procedures[0].source_digest,
        report.manifests[0].source_digest
    );
    assert_eq!(
        report.source_evidence.procedures[0].contract_hash,
        report.manifests[0].contract_hash
    );
}

#[test]
fn owner_direct_compile_reports_binding_phase_for_duplicate_inputs() {
    let diagnostic = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, ProductId i64) returns Reservation one (Reserved bool);",
    )
    .expect_err("duplicate inputs must remain a binder diagnostic in the owner pipeline");

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.message.contains("ProductId"));
}

fn signature_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);"
}

fn metadata(
    object_id: u64,
    procedure_id: u64,
    catalog_version: CatalogVersion,
) -> SrplProcedureContractMetadata {
    SrplProcedureContractMetadata {
        object_id: CatalogObjectId::new(object_id),
        procedure_id: ProcedureId::new(procedure_id),
        catalog_version,
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        structured_inputs: Vec::new(),
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
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
}
