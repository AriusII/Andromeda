#![forbid(unsafe_code)]

use andromeda_catalog::CatalogSystemStore;
use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectRef, CatalogSnapshot, ObjectKind, TableDefinition,
};
use andromeda_definition_batch::{
    DefinitionBatch, DefinitionBatchDependencyGraphHash, DefinitionBatchId,
    DefinitionBatchSourceHash, DefinitionOperation,
};
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    ResultStreamContract, StatsVersion, TransactionPolicy,
};
use andromeda_srpl_catalog_binding::bind_executable_procedure_plan;
use andromeda_srpl_ir::{
    Cardinality, SrplBusinessOperationIr, SrplBusinessOperationKindIr, SrplEmitValueIr,
    SrplPredicateIr, SrplProcedureBodyIr, SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};

const DB_ID: DatabaseId = DatabaseId::new(51);
const NS_ID: NamespaceId = NamespaceId::new(61);
const CATALOG_VERSION: CatalogVersion = CatalogVersion::new(1);

type CatalogPublicationReceipt = andromeda_catalog_store::CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

#[test]
fn owner_direct_catalog_snapshot_binding_builds_executable_plan_without_srpl_facade() {
    let snapshot = inventory_snapshot();
    let ir = procedure_ir();

    let plan = bind_executable_procedure_plan(&ir, &snapshot)
        .expect("catalog-binding owner must bind executable plans directly");

    assert_eq!(
        plan.procedure_name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(plan.body.operations.len(), 2);
    assert_eq!(plan.evidence.catalog_version, CATALOG_VERSION);
    assert_eq!(
        plan.evidence.procedure_object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(plan.evidence.bound_objects.len(), 1);
    assert_eq!(plan.evidence.bound_objects[0].kind, ObjectKind::Table);
}

#[test]
fn owner_direct_catalog_snapshot_binding_rejects_unknown_body_table() {
    let snapshot = inventory_snapshot();
    let mut ir = procedure_ir();
    let SrplBusinessOperationKindIr::Read { source, .. } = &mut ir.body.operations[0].kind else {
        panic!("first fixture operation must be a read");
    };
    *source = andromeda_catalog_store::QualifiedName::parse("Inventory.MissingStock").unwrap();

    let error = bind_executable_procedure_plan(&ir, &snapshot)
        .expect_err("unknown table reference must be rejected by catalog binding owner");

    assert_eq!(error.kind(), andromeda_error::AndromedaErrorKind::Catalog);
    assert!(error.message().contains("unbound table/object"));
}

fn inventory_snapshot() -> CatalogSnapshot<CatalogPublicationReceipt> {
    let table = TableDefinition {
        object: object(701, "Inventory.ProductStock", ObjectKind::Table),
        columns: vec![
            column("ProductId", ScalarType::I64, 0),
            column("AvailableQuantity", ScalarType::I64, 1),
        ],
    };
    let procedure = ProcedureContractCandidate {
        object: object(702, "Inventory.ReserveStock", ObjectKind::Procedure),
        procedure_id: ProcedureId::new(802),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xB1),
            frame_envelope_hash: ContractHash::test_vector(0xB2),
        },
        inputs: vec![column("ProductId", ScalarType::I64, 0)],
        structured_inputs: Vec::new(),
        result_streams: vec![ResultStreamContract {
            stream_id: 1,
            name: "Reservation".to_string(),
            columns: vec![column("Reserved", ScalarType::Bool, 0)],
            cardinality: Cardinality::One.into(),
            row_count_exact_required: true,
        }],
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
    .materialize()
    .unwrap();
    let batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(901),
        database_id: DB_ID,
        namespace_id: NS_ID,
        base_version: CatalogVersion::new(0),
        operations: vec![
            DefinitionOperation::Create(CatalogDefinition::Table(table)),
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure)),
        ],
    };
    let mut store = CatalogSystemStore::empty(DB_ID, NS_ID, CatalogVersion::new(0));
    let mut next_lsn: u64 = 0;
    store
        .apply_definition_batch_durably(
            &batch,
            |_kind, _payload| {
                next_lsn += 1;
                Ok(next_lsn)
            },
            Ok,
        )
        .unwrap();
    let published = store.into_snapshot();
    (*published).clone()
}

fn procedure_ir() -> SrplProcedureIr {
    SrplProcedureIr {
        name: andromeda_catalog_store::QualifiedName::parse("Inventory.ReserveStock").unwrap(),
        inputs: vec![column("ProductId", ScalarType::I64, 0)],
        result_streams: vec![SrplResultStreamIr {
            name: "Reservation".to_string(),
            cardinality: Cardinality::One,
            columns: vec![column("Reserved", ScalarType::Bool, 0)],
        }],
        body: SrplProcedureBodyIr {
            operations: vec![
                SrplBusinessOperationIr {
                    ordinal: 0,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: andromeda_catalog_store::QualifiedName::parse(
                            "Inventory.ProductStock",
                        )
                        .unwrap(),
                        binding: "Stock".to_string(),
                        cardinality: Cardinality::One,
                        predicates: vec![SrplPredicateIr::InputEqualsField {
                            input: "ProductId".to_string(),
                            binding: "Stock".to_string(),
                            field: "ProductId".to_string(),
                        }],
                    },
                },
                SrplBusinessOperationIr {
                    ordinal: 1,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: "Reservation".to_string(),
                        values: vec![SrplEmitValueIr {
                            column: "Reserved".to_string(),
                            value: SrplValueIr::bool(true),
                        }],
                    },
                },
            ],
        },
    }
}

fn object(id: u64, name: &str, kind: ObjectKind) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: andromeda_catalog_store::QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version: CATALOG_VERSION,
    }
}

fn column(name: &str, scalar_type: ScalarType, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(scalar_type),
        ordinal,
    }
}
