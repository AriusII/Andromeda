#![forbid(unsafe_code)]

use andromeda_catalog::QualifiedName;
use andromeda_srpl::{
    Cardinality, SrplAssignmentIr, SrplBusinessOperationIr, SrplBusinessOperationKindIr,
    SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr, SrplProcedureIr, SrplResultStreamIr,
    SrplValueIr,
    optimizer::{OptimizationLevel, optimize_procedure_ir, run_optimizer_pipeline},
    procedure_compiler::{
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_narrow_procedure_signature,
        inventory_reserve_stock_contract_metadata, lower_ir_to_contract_candidate,
    },
    procedure_model::{ArithOp, ConstantLiteral},
};
use andromeda_types::{CatalogVersion, ColumnDescriptor, ScalarType, TypeDescriptor};

fn qn(path: &str) -> QualifiedName {
    QualifiedName::parse(path).expect("valid qualified name")
}

fn int(value: i64) -> SrplValueIr {
    SrplValueIr::Constant(ConstantLiteral::Int64(value))
}

fn arith(op: ArithOp, left: SrplValueIr, right: SrplValueIr) -> SrplValueIr {
    SrplValueIr::BinaryArith {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn eq_pred(input: &str, binding: &str, field: &str) -> SrplPredicateIr {
    SrplPredicateIr::InputEqualsField {
        input: input.to_string(),
        binding: binding.to_string(),
        field: field.to_string(),
    }
}

fn assignment(field: &str, value: SrplValueIr) -> SrplAssignmentIr {
    SrplAssignmentIr {
        field: field.to_string(),
        value,
    }
}

fn procedure(operations: Vec<SrplBusinessOperationIr>) -> SrplProcedureIr {
    SrplProcedureIr {
        name: qn("Inventory.ReserveStock"),
        inputs: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        result_streams: vec![SrplResultStreamIr {
            name: "Reservation".to_string(),
            cardinality: Cardinality::One,
            columns: vec![ColumnDescriptor {
                name: "Reserved".to_string(),
                data_type: TypeDescriptor::required(ScalarType::Bool),
                ordinal: 0,
            }],
        }],
        body: SrplProcedureBodyIr { operations },
    }
}

fn read_op(ordinal: u32) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Read {
            source: qn("Inventory.ProductStock"),
            binding: "Stock".to_string(),
            cardinality: Cardinality::One,
            predicates: Vec::new(),
        },
    }
}

fn assert_op(ordinal: u32) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Assert {
            predicate: eq_pred("ProductId", "Stock", "ProductId"),
            failure_code: "InsufficientStock".to_string(),
        },
    }
}

fn update_op(
    ordinal: u32,
    target: &str,
    assignments: Vec<SrplAssignmentIr>,
) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Update {
            target: qn(target),
            predicates: vec![eq_pred("ProductId", "Stock", "ProductId")],
            assignments,
            affected_rows_exact: Some(1),
        },
    }
}

fn emit_op(ordinal: u32) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Emit {
            stream: "Reservation".to_string(),
            values: vec![SrplEmitValueIr {
                column: "Reserved".to_string(),
                value: SrplValueIr::Bool(true),
            }],
        },
    }
}

fn raise_op(ordinal: u32) -> SrplBusinessOperationIr {
    SrplBusinessOperationIr {
        ordinal,
        kind: SrplBusinessOperationKindIr::Raise {
            code: "ManualAbort".to_string(),
        },
    }
}

fn effectful_kinds(ir: &SrplProcedureIr) -> Vec<&'static str> {
    ir.body
        .operations
        .iter()
        .map(|operation| match &operation.kind {
            SrplBusinessOperationKindIr::Read { .. } => "read",
            SrplBusinessOperationKindIr::Assert { .. } => "assert",
            SrplBusinessOperationKindIr::Update { .. } => "update",
            SrplBusinessOperationKindIr::Emit { .. } => "emit",
            SrplBusinessOperationKindIr::Raise { .. } => "raise",
        })
        .collect()
}

fn update_assignment_fields(ir: &SrplProcedureIr, operation_index: usize) -> Vec<String> {
    match &ir.body.operations[operation_index].kind {
        SrplBusinessOperationKindIr::Update { assignments, .. } => assignments
            .iter()
            .map(|assignment| assignment.field.clone())
            .collect(),
        other => panic!("expected Update at {operation_index}, got {other:?}"),
    }
}

#[test]
fn optimizer_preserves_raise_when_predicate_pushdown_removes_neighboring_assert() {
    let ir = procedure(vec![read_op(0), assert_op(1), raise_op(2)]);

    let result = run_optimizer_pipeline(ir).expect("optimizer should preserve terminal raise");

    assert_eq!(
        effectful_kinds(&result.optimized_ir),
        vec!["read", "assert", "raise"]
    );
    assert!(result.optimized_ir.body.operations.iter().any(|operation| {
        matches!(
            &operation.kind,
            SrplBusinessOperationKindIr::Raise { code } if code == "ManualAbort"
        )
    }));
}

#[test]
fn safe_optimizer_preserves_mutation_and_emit_order_without_sorting_assignments() {
    let ir = procedure(vec![
        read_op(0),
        update_op(
            1,
            "Inventory.ProductStock",
            vec![
                assignment("ZQuantity", arith(ArithOp::Add, int(2), int(3))),
                assignment("AQuantity", int(1)),
            ],
        ),
        emit_op(2),
        update_op(
            3,
            "Inventory.ReservationAudit",
            vec![assignment("AuditFlag", SrplValueIr::Bool(true))],
        ),
    ]);

    let result = optimize_procedure_ir(ir.clone(), OptimizationLevel::Safe)
        .expect("safe optimizer must not reorder mutation surfaces");

    assert_eq!(effectful_kinds(&result.optimized_ir), effectful_kinds(&ir));
    assert_eq!(
        update_assignment_fields(&result.optimized_ir, 1),
        vec!["ZQuantity".to_string(), "AQuantity".to_string()]
    );
    assert_eq!(
        update_assignment_fields(&result.optimized_ir, 3),
        vec!["AuditFlag".to_string()]
    );
}

#[test]
fn aggressive_evidence_only_keeps_the_same_effect_surface_as_safe() {
    let ir = procedure(vec![
        read_op(0),
        update_op(
            1,
            "Inventory.ProductStock",
            vec![
                assignment("ZQuantity", arith(ArithOp::Multiply, int(6), int(7))),
                assignment("AQuantity", int(1)),
            ],
        ),
        raise_op(2),
    ]);

    let safe = optimize_procedure_ir(ir.clone(), OptimizationLevel::Safe)
        .expect("safe optimizer should accept bounded IR");
    let aggressive = optimize_procedure_ir(ir, OptimizationLevel::AggressiveEvidenceOnly)
        .expect("aggressive-evidence-only optimizer should remain evidence-safe");

    assert_eq!(
        effectful_kinds(&aggressive.optimized_ir),
        effectful_kinds(&safe.optimized_ir)
    );
    assert_eq!(
        update_assignment_fields(&aggressive.optimized_ir, 1),
        update_assignment_fields(&safe.optimized_ir, 1)
    );
}

#[test]
fn optimized_ir_still_lowers_with_contract_permissions_and_transaction_policy() {
    let ir = compile_narrow_procedure_signature(INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE)
        .expect("fixture source should compile");
    let metadata = inventory_reserve_stock_contract_metadata(CatalogVersion::new(42));
    let optimized = run_optimizer_pipeline(ir)
        .expect("fixture IR should optimize without changing contract metadata");

    let candidate =
        lower_ir_to_contract_candidate(optimized.optimized_ir, metadata.clone()).unwrap();

    assert_eq!(
        candidate.required_permissions,
        metadata.required_permissions
    );
    assert_eq!(candidate.transaction_policy, metadata.transaction_policy);
}
