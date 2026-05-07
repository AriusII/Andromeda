//! Comprehensive contract tests for SRPL result metadata extraction.
//!
//! This contract test suite verifies that the result metadata extraction logic
//! correctly handles all procedure kinds (SELECT/INSERT/UPDATE/DELETE/CALL) and
//! produces deterministic, valid metadata ready for emission before payload.
//!
//! Test Coverage:
//! - Extract metadata from EMIT operations with single/multiple values
//! - Extract metadata from READ operations with different cardinalities
//! - Extract metadata from UPDATE operations with exact/unknown row counts
//! - Validate cardinality inference from operation types
//! - Validate stream_id and column_count pass-through
//! - Validate row_count_exact and row_count_max are set correctly
//! - Validate deterministic extraction (same input -> same output)
//! - Validate error handling for invalid inputs

use andromeda_catalog::CatalogObjectRef;
use andromeda_catalog::ProcedureContractRef;
use andromeda_catalog::QualifiedName;
use andromeda_catalog::ResultStreamCardinality;
use andromeda_catalog::ResultStreamContract;
use andromeda_core::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};

use andromeda_exec::{DefaultResultMetadataExtractor, ResultMetadataExtractor};
use andromeda_srpl::{
    Cardinality,
    procedure_model::{
        BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan,
        SrplCatalogBindingEvidence, SrplEmitValueIr, SrplPredicateIr, SrplValueIr,
    },
};

// Test Helpers

fn make_catalog_version() -> CatalogVersion {
    CatalogVersion::new(1)
}

fn make_procedure_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(1),
        name: QualifiedName::parse("test.proc").unwrap(),
        kind: andromeda_catalog::ObjectKind::Procedure,
        catalog_version: make_catalog_version(),
    }
}

fn make_procedure_contract() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::new([1u8; 32]),
        catalog_version: make_catalog_version(),
    }
}

fn make_binding_evidence() -> SrplCatalogBindingEvidence {
    SrplCatalogBindingEvidence {
        catalog_version: make_catalog_version(),
        procedure_object: make_procedure_object(),
        procedure_contract: make_procedure_contract(),
        bound_objects: Vec::new(),
    }
}

fn make_result_stream_contract(stream_id: u64, columns: usize) -> ResultStreamContract {
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

fn make_table_ref() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(2),
        name: QualifiedName::parse("test.table").unwrap(),
        kind: andromeda_catalog::ObjectKind::Table,
        catalog_version: make_catalog_version(),
    }
}

// Contract Test Cases: 20+ tests

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
fn ct_007_extract_from_update_with_exact_rows() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![],
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
        assignments: vec![],
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
fn ct_009_extract_stream_id_pass_through() {
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
            value: SrplValueIr::Bool(true),
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
            value: SrplValueIr::Bool(true),
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

#[test]
fn ct_014_exact_count_sets_row_count_max() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![],
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
fn ct_015_multi_stream_selects_first() {
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
        assignments: vec![],
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
        assignments: vec![],
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

    // The metadata itself should pass validation
    let validation = metadata.validate_before_payload();
    assert!(
        validation.is_ok(),
        "metadata should validate before payload emission"
    );
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
fn ct_019_large_affected_rows_count() {
    let mut body = BoundSrplBodyPlan { operations: vec![] };
    let large_count = u64::MAX / 2;
    body.operations.push(BoundSrplOperationPlan::UpdateTable {
        ordinal: 0,
        target: make_table_ref(),
        predicates: vec![],
        assignments: vec![],
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
        assignments: vec![],
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
