#![forbid(unsafe_code)]

use andromeda_optimizer::srpl::{
    OptimizationLevel, OptimizerDecisionKind, optimize_procedure_ir, phase::OptimizerPhase,
    plan_kind::OptimizerPlanKind, run_optimizer_pipeline,
};
use andromeda_procedure_contract::QualifiedName;
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
fn pipeline_emits_decision_trace_with_per_alternative_cost() {
    // Build a minimal single-read procedure that yields a PointLookup plan.
    let ir = procedure(vec![
        SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("Inventory.ProductStock"),
                binding: "Stock".to_string(),
                cardinality: Cardinality::One,
                predicates: vec![eq_pred("ProductId", "Stock", "ProductId")],
            },
        },
        SrplBusinessOperationIr {
            ordinal: 1,
            kind: SrplBusinessOperationKindIr::Emit {
                stream: "Reservation".to_string(),
                values: vec![SrplEmitValueIr {
                    column: "Reserved".to_string(),
                    value: SrplValueIr::bool(true),
                }],
            },
        },
    ]);

    let result = run_optimizer_pipeline(ir).expect("pipeline should succeed");

    // The breakdown must be non-empty (one alternative = the chosen plan).
    assert!(
        !result.alternative_cost_breakdown.is_empty(),
        "alternative_cost_breakdown must not be empty after pipeline"
    );

    // Exactly one alternative (the current pipeline produces one candidate).
    assert_eq!(
        result.alternative_cost_breakdown.len(),
        1,
        "expected exactly one alternative in the breakdown"
    );

    let alt = &result.alternative_cost_breakdown[0];

    // The single alternative must be marked chosen.
    assert!(
        alt.chosen(),
        "the sole alternative must be marked chosen: plan_kind={}",
        alt.plan_kind()
    );

    // Rejection is absent for the chosen plan.
    assert!(
        alt.rejection().is_none(),
        "chosen alternative must have no rejection reason"
    );

    // The plan kind label must be non-empty and stable.
    assert!(
        !alt.plan_kind().is_empty(),
        "plan_kind label must not be empty"
    );
    assert_eq!(
        alt.plan_kind(),
        "point-lookup",
        "expected point-lookup plan kind"
    );

    // All cost components must be non-negative finite values.
    let cost = alt.cost();
    assert!(
        cost.cpu_cost.is_finite() && cost.cpu_cost >= 0.0,
        "cpu_cost must be finite and non-negative"
    );
    assert!(
        cost.logical_io_cost.is_finite() && cost.logical_io_cost >= 0.0,
        "logical_io_cost must be finite and non-negative"
    );
    assert!(
        cost.physical_io_cost.is_finite() && cost.physical_io_cost >= 0.0,
        "physical_io_cost must be finite and non-negative"
    );
    assert!(
        cost.total_cost.is_finite() && cost.total_cost >= 0.0,
        "total_cost must be finite and non-negative"
    );

    // The breakdown cost must match the pipeline's chosen_cost.
    assert!(
        (cost.total_cost - result.chosen_cost.total_cost).abs() < 1e-9,
        "breakdown total_cost={} must match chosen_cost={}",
        cost.total_cost,
        result.chosen_cost.total_cost
    );
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
