use crate::support::*;

#[test]
fn ct_003_extract_from_read_one_cardinality() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::ReadTable {
        ordinal: 0,
        source: make_table_ref(),
        binding: "t".to_string(),
        cardinality: Cardinality::One,
        predicates: vec![],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let metadata = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 1)],
    )
    .expect("extraction should succeed");

    assert_eq!(metadata.cardinality, Cardinality::One);
}
#[test]
fn ct_004_extract_from_read_optional_one_cardinality() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::ReadTable {
        ordinal: 0,
        source: make_table_ref(),
        binding: "t".to_string(),
        cardinality: Cardinality::OptionalOne,
        predicates: vec![],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let metadata = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 1)],
    )
    .expect("OptionalOne read metadata may be emitted without RowCountExact");

    assert_eq!(metadata.cardinality, Cardinality::OptionalOne);
    assert_eq!(metadata.row_count_exact, None);
    assert_eq!(metadata.row_count_max, None);
}
#[test]
fn ct_005_extract_from_read_many_cardinality() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::ReadTable {
        ordinal: 0,
        source: make_table_ref(),
        binding: "t".to_string(),
        cardinality: Cardinality::Many,
        predicates: vec![],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let metadata = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 1)],
    )
    .expect("extraction should succeed");

    assert_eq!(metadata.cardinality, Cardinality::Many);
}
#[test]
fn ct_006_extract_from_read_nonempty_many_cardinality() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::ReadTable {
        ordinal: 0,
        source: make_table_ref(),
        binding: "t".to_string(),
        cardinality: Cardinality::NonEmptyMany,
        predicates: vec![],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let result = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 1)],
    );

    assert!(
        result.is_err(),
        "NonEmptyMany read metadata must fail closed until RowCountExact is available"
    );
}
#[test]
fn ct_013_skip_assert_operations() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };

    body.operations.push(BoundSrplOperationPlan::Assert {
        ordinal: 0,
        predicate: SrplPredicateIr::InputEqualsField {
            input: "x".to_string(),
            binding: "t".to_string(),
            field: "f".to_string(),
        },
        failure_code: "assertion_failed".to_string(),
    });

    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 1,
        stream: "result".to_string(),
        values: vec![SrplEmitValueIr {
            column: "col_0".to_string(),
            value: SrplValueIr::Bool(true),
        }],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let metadata = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 1)],
    )
    .expect("extraction should succeed");

    // Should extract from EMIT, skipping ASSERT
    assert_eq!(metadata.row_count_exact, Some(1));
    assert_eq!(metadata.cardinality, Cardinality::One);
}
