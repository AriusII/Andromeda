#![forbid(unsafe_code)]

use andromeda_contract::QualifiedName;
use andromeda_optimizer::srpl::{
    OptimizationLevel, OptimizerDecisionKind, optimize_procedure_ir, phase::OptimizerPhase,
    plan_kind::OptimizerPlanKind, run_optimizer_pipeline,
};
use andromeda_srpl_ir::{
    ArithOp, Cardinality, ConstantLiteral, SrplAssignmentIr, SrplBusinessOperationIr,
    SrplBusinessOperationKindIr, SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr,
    SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
};
use andromeda_types::{ColumnDescriptor, ScalarType, TypeDescriptor};

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

#[test]
fn safe_pipeline_orchestrates_existing_passes_and_returns_plan_evidence() {
    let pushed_predicate = eq_pred("ProductId", "Stock", "ProductId");
    let ir = procedure(vec![
        SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("Inventory.ProductStock"),
                binding: "Stock".to_string(),
                cardinality: Cardinality::One,
                predicates: vec![],
            },
        },
        SrplBusinessOperationIr {
            ordinal: 1,
            kind: SrplBusinessOperationKindIr::Assert {
                predicate: pushed_predicate.clone(),
                failure_code: "InsufficientStock".to_string(),
            },
        },
        SrplBusinessOperationIr {
            ordinal: 2,
            kind: SrplBusinessOperationKindIr::Update {
                target: qn("Inventory.ProductStock"),
                predicates: vec![],
                assignments: vec![SrplAssignmentIr {
                    field: "AvailableQuantity".to_string(),
                    value: arith(ArithOp::Subtract, int(10), int(3)),
                }],
                affected_rows_exact: Some(1),
            },
        },
        SrplBusinessOperationIr {
            ordinal: 3,
            kind: SrplBusinessOperationKindIr::Emit {
                stream: "Reservation".to_string(),
                values: vec![SrplEmitValueIr {
                    column: "Reserved".to_string(),
                    value: SrplValueIr::bool(true),
                }],
            },
        },
    ]);

    let optimized = run_optimizer_pipeline(ir).expect("safe pipeline should optimize bounded IR");

    assert_eq!(
        optimized.phases,
        vec![
            OptimizerPhase::ConstantFolding,
            OptimizerPhase::PredicatePushdown,
            OptimizerPhase::Normalize,
            OptimizerPhase::ProjectionPushdown,
            OptimizerPhase::CostAnalysis,
            OptimizerPhase::PlanChoice,
        ]
    );
    assert_eq!(optimized.optimized_ir.body.operations.len(), 4);
    assert_eq!(optimized.chosen_kind, OptimizerPlanKind::PointLookup);
    assert!(optimized.chosen_cost.is_valid());
    assert_eq!(optimized.alternatives.len(), 1);
    assert!(optimized.alternatives[0].chosen);
    assert_eq!(optimized.projections.len(), 1);
    assert!(
        !optimized.diagnostics.iter().any(|diagnostic| {
            diagnostic.decision == OptimizerDecisionKind::PredicatePushedToRead
        })
    );
    assert!(optimized.diagnostics.iter().any(|diagnostic| {
        diagnostic.decision == OptimizerDecisionKind::ConstantValueFolded
            && diagnostic.source_operation_ordinal == Some(2)
            && diagnostic.resulting_operation_ordinal == Some(2)
    }));

    match &optimized.optimized_ir.body.operations[0].kind {
        SrplBusinessOperationKindIr::Read { predicates, .. } => {
            assert!(predicates.is_empty());
        },
        other => panic!("expected Read, got {other:?}"),
    }

    match &optimized.optimized_ir.body.operations[1].kind {
        SrplBusinessOperationKindIr::Assert {
            predicate,
            failure_code,
        } => {
            assert_eq!(predicate, &pushed_predicate);
            assert_eq!(failure_code, "InsufficientStock");
        },
        other => panic!("expected Assert, got {other:?}"),
    }

    match &optimized.optimized_ir.body.operations[2].kind {
        SrplBusinessOperationKindIr::Update { assignments, .. } => {
            assert_eq!(assignments[0].value, int(7));
        },
        other => panic!("expected Update, got {other:?}"),
    }

    match &optimized.optimized_ir.body.operations[3].kind {
        SrplBusinessOperationKindIr::Emit { values, .. } => {
            assert_eq!(
                values[0].value,
                SrplValueIr::Constant(ConstantLiteral::Bool(true))
            );
        },
        other => panic!("expected Emit, got {other:?}"),
    }
}

#[test]
fn no_optimization_level_preserves_ir_shape_but_still_emits_evidence() {
    let assert_predicate = eq_pred("ProductId", "Stock", "ProductId");
    let ir = procedure(vec![
        SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("Inventory.ProductStock"),
                binding: "Stock".to_string(),
                cardinality: Cardinality::One,
                predicates: vec![],
            },
        },
        SrplBusinessOperationIr {
            ordinal: 1,
            kind: SrplBusinessOperationKindIr::Assert {
                predicate: assert_predicate,
                failure_code: "InsufficientStock".to_string(),
            },
        },
        SrplBusinessOperationIr {
            ordinal: 2,
            kind: SrplBusinessOperationKindIr::Raise {
                code: "ManualAbort".to_string(),
            },
        },
    ]);

    let result = optimize_procedure_ir(ir.clone(), OptimizationLevel::None)
        .expect("none level should still produce cost and plan evidence");

    assert_eq!(result.optimized_ir, ir);
    assert_eq!(
        result.phases,
        vec![
            OptimizerPhase::ProjectionPushdown,
            OptimizerPhase::CostAnalysis,
            OptimizerPhase::PlanChoice,
        ]
    );
    assert!(matches!(
        result.optimized_ir.body.operations[1].kind,
        SrplBusinessOperationKindIr::Assert { .. }
    ));
    assert!(matches!(
        result.optimized_ir.body.operations[2].kind,
        SrplBusinessOperationKindIr::Raise { .. }
    ));
}
