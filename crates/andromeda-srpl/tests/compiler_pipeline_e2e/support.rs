pub(crate) use andromeda_catalog::{
    AccessMode, CatalogDefinition, CatalogObjectRef, CatalogSnapshot, CompatibilityPolicy,
    DefinitionBatch, DefinitionBatchId, DefinitionOperation, INVENTORY_DATABASE_ID,
    INVENTORY_DEFINITION_BATCH_ID, INVENTORY_NAMESPACE_ID, IsolationPolicy, MultiResultPolicy,
    ObjectKind, ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract, StatsVersion,
    StructuredObjectDefinition, TransactionPolicy, inventory_domain_definition_batch,
    inventory_protocol_layout_ref, inventory_reserve_stock_contract,
    inventory_reserve_stock_contract_candidate,
};
pub(crate) use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash,
    ProcedureId, ScalarType, TypeDescriptor,
};
pub(crate) use andromeda_srpl::{
    DiagnosticPhase, SourceSpan,
    procedure_compiler::{
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, bind_executable_procedure_plan,
        compile_inventory_reserve_stock_contract,
        compile_inventory_reserve_stock_contract_candidate,
        compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
        compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
        inventory_reserve_stock_body_ir, inventory_reserve_stock_contract_metadata,
        lower_ir_to_catalog_definition, lower_ir_to_contract_candidate, parse_procedure_signature,
    },
    procedure_model::{
        Cardinality, ProcedureSignature, ResultContract, SrplBusinessOperationIr,
        SrplBusinessOperationKindIr, SrplEmitValueIr, SrplProcedureBodyIr,
        SrplProcedureContractMetadata, SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
    },
    source_location::SrplSource,
};

pub(crate) fn contract_metadata() -> SrplProcedureContractMetadata {
    SrplProcedureContractMetadata {
        object_id: CatalogObjectId::new(11),
        procedure_id: ProcedureId::new(11),
        catalog_version: CatalogVersion::new(3),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        structured_inputs: vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
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
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

pub(crate) fn stock_request_structured_object(
    catalog_version: CatalogVersion,
) -> StructuredObjectDefinition {
    StructuredObjectDefinition {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(0x51_00),
            name: QualifiedName::parse("Inventory.StockRequest").unwrap(),
            kind: ObjectKind::StructuredObject,
            catalog_version,
        },
        fields: vec![
            ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            },
            ColumnDescriptor {
                name: "Quantity".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 1,
            },
        ],
        unique_by: vec!["ProductId".to_string()],
    }
}

pub(crate) fn inventory_catalog_snapshot() -> CatalogSnapshot {
    let batch = inventory_domain_definition_batch().unwrap();
    let plan = batch.dry_run().unwrap();
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();
    snapshot
}

pub(crate) fn cardinality_probe_snapshot(
    procedure_name: &str,
    result_name: &str,
    cardinality: Cardinality,
    emit_count: usize,
) -> (SrplProcedureIr, CatalogSnapshot) {
    let catalog_version = CatalogVersion::new(1);
    let result_columns = vec![ColumnDescriptor {
        name: "Present".to_string(),
        data_type: TypeDescriptor::required(ScalarType::Bool),
        ordinal: 0,
    }];
    let contract = ProcedureContractCandidate {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(0x7100 + emit_count as u64),
            name: QualifiedName::parse(procedure_name).unwrap(),
            kind: ObjectKind::Procedure,
            catalog_version,
        },
        procedure_id: ProcedureId::new(0x7200 + emit_count as u64),
        stats_version: StatsVersion::new(1),
        protocol_layout: inventory_protocol_layout_ref(),
        inputs: Vec::new(),
        structured_inputs: Vec::new(),
        result_streams: vec![ResultStreamContract {
            stream_id: 1,
            name: result_name.to_string(),
            columns: result_columns.clone(),
            cardinality: cardinality.into(),
            row_count_exact_required: cardinality.requires_exact_row_count(),
        }],
        required_permissions: vec![format!("{procedure_name}.Execute")],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadOnly,
            isolation: IsolationPolicy::Serializable,
            retryable: true,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: false,
            allowed_error_codes: vec!["NoValue".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
    .materialize()
    .unwrap();

    let batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0x7300 + emit_count as u64),
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version: CatalogVersion::new(0),
        operations: vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            contract,
        ))],
    };
    let plan = batch.dry_run().unwrap();
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();

    let operations = if emit_count == 0 {
        vec![SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Raise {
                code: "NoValue".to_string(),
            },
        }]
    } else {
        (0..emit_count)
            .map(|ordinal| SrplBusinessOperationIr {
                ordinal: ordinal as u32,
                kind: SrplBusinessOperationKindIr::Emit {
                    stream: result_name.to_string(),
                    values: vec![SrplEmitValueIr {
                        column: "Present".to_string(),
                        value: SrplValueIr::Bool(true),
                    }],
                },
            })
            .collect()
    };

    let ir = SrplProcedureIr {
        name: QualifiedName::parse(procedure_name).unwrap(),
        inputs: Vec::new(),
        result_streams: vec![SrplResultStreamIr {
            name: result_name.to_string(),
            cardinality,
            columns: result_columns,
        }],
        body: SrplProcedureBodyIr { operations },
    };

    (ir, snapshot)
}
