pub use andromeda_execution::{
    FieldValue, SrplExecutionAdapter, SrplStreamBackpressure, SrplTransactionContext,
    SrplTypedEnvironment, StructuredObject,
};
pub use andromeda_procedure_contract::{
    CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName,
};
pub use andromeda_srpl_execution_adapter::{
    SrplAssertRequest, SrplEmitRequest, SrplExecutionFailure, SrplFailureRequest,
    SrplOperationContext, SrplReadRequest, SrplRowBound, SrplUpdateRequest,
};
pub use andromeda_srpl_ir::{
    Cardinality, SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr, SrplValueIr,
};
pub use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

// HELPER FUNCTIONS

pub(crate) fn make_test_procedure_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(1),
        catalog_version: CatalogVersion::new(1),
    }
}

pub(crate) fn make_test_object_ref(name: &str) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(1),
        name: QualifiedName::parse(name).unwrap(),
        kind: ObjectKind::Table,
        catalog_version: CatalogVersion::new(1),
    }
}

pub(crate) fn make_read_request(
    ordinal: u32,
    row_bound: SrplRowBound,
    cardinality: Cardinality,
) -> SrplReadRequest {
    SrplReadRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), ordinal).unwrap(),
        make_test_object_ref("test.schema.table"),
        cardinality,
        row_bound,
        vec![],
    )
    .unwrap()
}

pub(crate) fn make_assert_request(ordinal: u32) -> SrplAssertRequest {
    let predicate = SrplPredicateIr::InputEqualsField {
        input: "expected_id".to_string(),
        binding: "data".to_string(),
        field: "actual_id".to_string(),
    };

    SrplAssertRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), ordinal).unwrap(),
        predicate,
        "check_failed",
    )
    .unwrap()
}

pub(crate) fn emit_values() -> Vec<SrplEmitValueIr> {
    vec![SrplEmitValueIr {
        column: "ok".to_string(),
        value: SrplValueIr::bool(true),
    }]
}

pub(crate) fn make_update_request(ordinal: u32) -> SrplUpdateRequest {
    SrplUpdateRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), ordinal).unwrap(),
        make_test_object_ref("test.schema.table"),
        SrplRowBound::exact(1).unwrap(),
        vec![],
        vec![SrplAssignmentIr {
            field: "status".to_string(),
            value: SrplValueIr::bool(true),
        }],
    )
    .unwrap()
}

pub(crate) fn make_emit_request(ordinal: u32) -> SrplEmitRequest {
    SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), ordinal).unwrap(),
        "result_stream",
        Cardinality::Many,
        SrplRowBound::at_most(10).unwrap(),
        emit_values(),
    )
    .unwrap()
}

pub(crate) fn make_failure_request(
    ordinal: u32,
    failure: SrplExecutionFailure,
) -> SrplFailureRequest {
    SrplFailureRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), ordinal).unwrap(),
        "FAILURE",
        failure,
    )
    .unwrap()
}
