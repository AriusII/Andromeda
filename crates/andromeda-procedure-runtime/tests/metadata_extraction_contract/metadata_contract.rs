use crate::support::*;

#[test]
fn ct_009_extract_stream_id_pass_through() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
        stream: "result".to_string(),
        values: vec![SrplEmitValueIr {
            column: "col_0".to_string(),
            value: SrplValueIr::bool(true),
        }],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let stream_ids = [999, 1000, 12345, u64::MAX];
    for stream_id in stream_ids {
        let metadata = DefaultResultMetadataExtractor::extract_metadata(
            &plan,
            &[make_result_stream_contract(stream_id, 1)],
        )
        .expect("extraction should succeed");

        assert_eq!(metadata.stream_id, stream_id);
    }
}
#[test]
fn ct_010_extract_column_count_pass_through() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    let emit_values = (0..10)
        .map(|i| SrplEmitValueIr {
            column: format!("col_{}", i),
            value: SrplValueIr::bool(true),
        })
        .collect();

    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
        stream: "result".to_string(),
        values: emit_values,
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let column_counts = [1, 5, 10, 100];
    for column_count in column_counts {
        let metadata = DefaultResultMetadataExtractor::extract_metadata(
            &plan,
            &[make_result_stream_contract(1, column_count)],
        )
        .expect("extraction should succeed");

        assert_eq!(metadata.column_count, column_count as u32);
    }
}
#[test]
fn ct_011_reject_empty_result_streams() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
        stream: "result".to_string(),
        values: vec![SrplEmitValueIr {
            column: "col_0".to_string(),
            value: SrplValueIr::bool(true),
        }],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let result = DefaultResultMetadataExtractor::extract_metadata(&plan, &[]);
    assert!(result.is_err(), "should reject empty result streams");
}
#[test]
fn ct_012_reject_empty_procedure_body() {
    let body = BoundSrplBodyPlan { operations: vec![] };

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let result = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 1)],
    );
    assert!(result.is_err(), "should reject empty procedure body");
}
#[test]
fn ct_015_multi_stream_selects_first() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
        stream: "result".to_string(),
        values: vec![SrplEmitValueIr {
            column: "col_0".to_string(),
            value: SrplValueIr::bool(true),
        }],
    });

    let plan = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body,
        evidence: make_binding_evidence(),
    };

    let stream1 = make_result_stream_contract(1, 1);
    let stream2 = make_result_stream_contract(2, 2);
    let stream3 = make_result_stream_contract(3, 3);

    let metadata =
        DefaultResultMetadataExtractor::extract_metadata(&plan, &[stream1, stream2, stream3])
            .expect("extraction should succeed");

    // Should use first stream's properties
    assert_eq!(metadata.stream_id, 1);
    assert_eq!(metadata.column_count, 1);
}
#[test]
fn ct_016_deterministic_extraction_same_input() {
    let mut body1 = BoundSrplBodyPlan { operations: vec![] };
    body1.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: Some(999),
    });

    let plan1 = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body: body1,
        evidence: make_binding_evidence(),
    };

    let mut body2 = BoundSrplBodyPlan { operations: vec![] };
    body2.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: Some(999),
    });

    let plan2 = ExecutableProcedurePlan {
        procedure_name: QualifiedName::parse("test.proc").unwrap(),
        body: body2,
        evidence: make_binding_evidence(),
    };

    let stream = make_result_stream_contract(1, 1);
    let metadata1 =
        DefaultResultMetadataExtractor::extract_metadata(&plan1, std::slice::from_ref(&stream))
            .expect("extraction should succeed");
    let metadata2 =
        DefaultResultMetadataExtractor::extract_metadata(&plan2, std::slice::from_ref(&stream))
            .expect("extraction should succeed");

    assert_eq!(
        metadata1, metadata2,
        "same input should produce identical metadata"
    );
}
#[test]
fn ct_017_metadata_validates_before_payload() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
        stream: "result".to_string(),
        values: vec![SrplEmitValueIr {
            column: "col_0".to_string(),
            value: SrplValueIr::bool(true),
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

    // The metadata itself should pass validation
    let validation = metadata.validate_before_payload();
    assert!(
        validation.is_ok(),
        "metadata should validate before payload emission"
    );
}
