use crate::support::*;

#[test]
fn ct_001_extract_from_single_value_emit() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
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

    assert_eq!(metadata.stream_id, 1);
    assert_eq!(metadata.row_count_exact, Some(1));
    assert_eq!(metadata.column_count, 1);
    assert_eq!(metadata.cardinality, Cardinality::One);
}
#[test]
fn ct_002_extract_from_multi_column_emit_is_one_result_row() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    let emit_values = (0..3)
        .map(|i| SrplEmitValueIr {
            column: format!("col_{}", i),
            value: SrplValueIr::Bool(i % 2 == 0),
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

    let metadata = DefaultResultMetadataExtractor::extract_metadata(
        &plan,
        &[make_result_stream_contract(1, 3)],
    )
    .expect("extraction should succeed");

    assert_eq!(metadata.row_count_exact, Some(1));
    assert_eq!(metadata.cardinality, Cardinality::One);
    assert_eq!(metadata.column_count, 3);
}
#[test]
fn ct_018_emit_value_types_dont_affect_cardinality() {
    let value_types = vec![
        SrplValueIr::Bool(true),
        SrplValueIr::Input("x".to_string()),
        SrplValueIr::Field {
            binding: "t".to_string(),
            field: "f".to_string(),
        },
        SrplValueIr::SubtractInput {
            binding: "t".to_string(),
            field: "f".to_string(),
            input: "x".to_string(),
        },
    ];

    for value in value_types {
        let mut body = BoundSrplBodyPlan { operations: vec![] };
        body.operations.push(BoundSrplOperationPlan::Emit {
            ordinal: 0,
            stream: "result".to_string(),
            values: vec![SrplEmitValueIr {
                column: "col_0".to_string(),
                value,
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

        // All should produce One cardinality (single value per emit)
        assert_eq!(
            metadata.cardinality,
            Cardinality::One,
            "single emit value should produce One cardinality"
        );
    }
}
#[test]
fn ct_020_emit_then_read_uses_first_operation() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };

    // Add EMIT first
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
        stream: "result".to_string(),
        values: vec![SrplEmitValueIr {
            column: "col_0".to_string(),
            value: SrplValueIr::Bool(true),
        }],
    });

    // Add READ after (should not be used)
    body.operations.push(BoundSrplOperationPlan::ReadTable {
        ordinal: 1,
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

    // Should use EMIT's row count (1), not READ's (Many/Unknown)
    assert_eq!(metadata.row_count_exact, Some(1));
    assert_eq!(metadata.cardinality, Cardinality::One);
}
#[test]
fn ct_021_read_then_emit_uses_named_emit_result_shape() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };

    body.operations.push(BoundSrplOperationPlan::ReadTable {
        ordinal: 0,
        source: make_table_ref(),
        binding: "t".to_string(),
        cardinality: Cardinality::Many,
        predicates: vec![],
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
    .expect("extraction should use the named result emit");

    assert_eq!(metadata.row_count_exact, Some(1));
    assert_eq!(metadata.row_count_max, Some(1));
    assert_eq!(metadata.cardinality, Cardinality::One);
}
#[test]
fn ct_022_update_then_emit_does_not_use_affected_rows_as_result_rows() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };

    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![make_assignment()],
        affected_rows_exact: Some(42),
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
    .expect("extraction should use the named result emit");

    assert_eq!(metadata.row_count_exact, Some(1));
    assert_eq!(metadata.row_count_max, Some(1));
    assert_eq!(metadata.cardinality, Cardinality::One);
}
#[test]
fn ct_023_metadata_includes_all_required_fields() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::Emit {
        ordinal: 0,
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
        &[make_result_stream_contract(42, 5)],
    )
    .expect("extraction should succeed");

    // Verify all fields are set
    assert_eq!(metadata.stream_id, 42);
    assert!(metadata.row_count_exact.is_some());
    assert_eq!(metadata.column_count, 5);
    assert_ne!(metadata.cardinality, Cardinality::Many); // Should be specific, not Many
}
