pub use andromeda_catalog_store::{CatalogObjectRef, ObjectKind, QualifiedName};
pub use andromeda_core::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};

pub use andromeda_exec::{DefaultResultMetadataExtractor, ResultMetadataExtractor};
pub use andromeda_procedure_contract::{
    ProcedureContractRef, ResultStreamCardinality, ResultStreamContract,
};
pub use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, Cardinality, ConstantLiteral,
    ExecutableProcedurePlan, SrplAssignmentIr, SrplCatalogBindingEvidence, SrplEmitValueIr,
    SrplPredicateIr, SrplValueIr,
};

// Test Helpers

pub(crate) fn make_catalog_version() -> CatalogVersion {
    CatalogVersion::new(1)
}

pub(crate) fn make_procedure_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(1),
        name: QualifiedName::parse("test.proc").unwrap(),
        kind: ObjectKind::Procedure,
        catalog_version: make_catalog_version(),
    }
}

pub(crate) fn make_procedure_contract() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::new([1u8; 32]),
        catalog_version: make_catalog_version(),
    }
}

pub(crate) fn make_binding_evidence() -> SrplCatalogBindingEvidence {
    SrplCatalogBindingEvidence {
        catalog_version: make_catalog_version(),
        procedure_object: make_procedure_object(),
        procedure_contract: make_procedure_contract(),
        bound_objects: Vec::new(),
    }
}

pub(crate) fn make_result_stream_contract(stream_id: u64, columns: usize) -> ResultStreamContract {
    ResultStreamContract {
        stream_id,
        name: "result".to_string(),
        columns: (0..columns)
            .map(|i| ColumnDescriptor {
                name: format!("col_{}", i),
                ordinal: i as u32,
                data_type: TypeDescriptor::required(ScalarType::I64),
            })
            .collect(),
        cardinality: ResultStreamCardinality::Many,
        row_count_exact_required: false,
    }
}

pub(crate) fn make_table_ref() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(2),
        name: QualifiedName::parse("test.table").unwrap(),
        kind: ObjectKind::Table,
        catalog_version: make_catalog_version(),
    }
}

pub(crate) fn make_assignment() -> SrplAssignmentIr {
    SrplAssignmentIr {
        field: "quantity".to_string(),
        value: SrplValueIr::Constant(ConstantLiteral::Int64(1)),
    }
}
