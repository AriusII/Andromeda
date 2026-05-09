use crate::support::*;

#[test]
fn ct_007_extract_from_update_with_exact_rows() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: Some(42),
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

    assert_eq!(metadata.row_count_exact, Some(42));
    assert_eq!(metadata.row_count_max, Some(42));
}
#[test]
fn ct_008_extract_from_update_without_exact_rows() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: None,
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

    assert_eq!(metadata.row_count_exact, None);
    assert_eq!(metadata.row_count_max, None);
}
#[test]
fn ct_014_exact_count_sets_row_count_max() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: Some(100),
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

    assert_eq!(metadata.row_count_exact, Some(100));
    assert_eq!(metadata.row_count_max, Some(100));
    assert_eq!(metadata.row_count_exact, metadata.row_count_max);
}
#[test]
fn ct_019_large_affected_rows_count() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    let large_count = u64::MAX / 2;
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: Some(large_count),
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

    assert_eq!(metadata.row_count_exact, Some(large_count));
    assert_eq!(metadata.row_count_max, Some(large_count));
}
