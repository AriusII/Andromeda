#![forbid(unsafe_code)]

use andromeda_contract::QualifiedName;
use andromeda_srpl::{
    Cardinality, DiagnosticPhase, SrplAssignmentIr, SrplBusinessOperationIr,
    SrplBusinessOperationKindIr, SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr,
    SrplProcedureIr, SrplResultStreamIr, SrplValueIr,
    optimizer::{
        OptimizationLevel, OptimizerDecisionKind, OptimizerPipelineConfig,
        optimize_procedure_ir_with_config, phase::OptimizerPhase,
    },
    procedure_compiler::compile_narrow_procedure_signature_with_optimizer,
    procedure_model::{ArithOp, ConstantLiteral},
};
use andromeda_types::{ColumnDescriptor, ScalarType, TypeDescriptor};

fn qn(path: &str) -> QualifiedName {
    QualifiedName::parse(path).expect("qualified name")
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
        name: qn("Inventory.QueryStock"),
        inputs: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        result_streams: vec![SrplResultStreamIr {
            name: "Snapshot".to_string(),
            cardinality: Cardinality::One,
            columns: vec![ColumnDescriptor {
                name: "AvailableQuantity".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
        }],
        body: SrplProcedureBodyIr { operations },
    }
}

fn byte_line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[test]
fn optimized_compile_pipeline_preserves_binding_source_span() {
    let source = "\
procedure Inventory.ReserveStock
accepts (
  ProductId i64,
  ProductId i64
)
returns Reservation one (Reserved bool);";

    let diagnostic = compile_narrow_procedure_signature_with_optimizer(
        source,
        OptimizerPipelineConfig::default(),
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    let span = diagnostic
        .location
        .expect("duplicate parameter diagnostic keeps source span");
    assert_eq!(&source[span.start..span.end], "ProductId");
    assert_eq!(byte_line_column(source, span.start), (4, 3));
    assert!(diagnostic.message.contains("input names must be unique"));
    assert!(
        diagnostic
            .message
            .contains("procedure Inventory.ReserveStock")
    );
    assert!(diagnostic.message.contains("parameter ProductId"));
    assert!(diagnostic.message.contains("line 4, column 3"));
}

#[test]
fn optimized_compile_pipeline_records_parse_bind_lower_and_optimizer_phases() {
    let source = "\
procedure Inventory.QueryStock
accepts (ProductId i64)
returns Snapshot one (AvailableQuantity i64)
body {
  read Inventory.ProductStock Stock one;
  assert ProductId InsufficientStock;
  emit Snapshot (AvailableQuantity);
};";

    let result = compile_narrow_procedure_signature_with_optimizer(
        source,
        OptimizerPipelineConfig::new(OptimizationLevel::None).with_noop_decisions(true),
    )
    .expect("none optimizer level should run after parse, bind, and lower");

    assert_eq!(
        &result.phases[..3],
        &[
            OptimizerPhase::Parsing,
            OptimizerPhase::Binding,
            OptimizerPhase::IRLowering,
        ]
    );
    assert!(result.phases.contains(&OptimizerPhase::ProjectionPushdown));
    assert!(result.phases.contains(&OptimizerPhase::CostAnalysis));
    assert!(result.phases.contains(&OptimizerPhase::PlanChoice));

    let safe = compile_narrow_procedure_signature_with_optimizer(
        source,
        OptimizerPipelineConfig::new(OptimizationLevel::Safe),
    )
    .expect("safe optimizer level should run after parse, bind, and lower");
    assert_eq!(
        &safe.phases[..6],
        &[
            OptimizerPhase::Parsing,
            OptimizerPhase::Binding,
            OptimizerPhase::IRLowering,
            OptimizerPhase::ConstantFolding,
            OptimizerPhase::PredicatePushdown,
            OptimizerPhase::Normalize,
        ]
    );
}

#[test]
fn optimizer_diagnostics_preserve_operation_provenance_through_fold_and_normalize() {
    let duplicated = eq_pred("ProductId", "Stock", "ProductId");
    let unsorted = eq_pred("Alpha", "Stock", "Alpha");
    let ir = procedure(vec![
        SrplBusinessOperationIr {
            ordinal: 0,
            kind: SrplBusinessOperationKindIr::Read {
                source: qn("Inventory.ProductStock"),
                binding: "Stock".to_string(),
                cardinality: Cardinality::One,
                predicates: vec![duplicated.clone(), unsorted, duplicated],
            },
        },
        SrplBusinessOperationIr {
            ordinal: 1,
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
            ordinal: 2,
            kind: SrplBusinessOperationKindIr::Emit {
                stream: "Snapshot".to_string(),
                values: vec![SrplEmitValueIr {
                    column: "AvailableQuantity".to_string(),
                    value: SrplValueIr::Bool(true),
                }],
            },
        },
    ]);

    let result = optimize_procedure_ir_with_config(ir, OptimizerPipelineConfig::default())
        .expect("safe optimizer should preserve provenance");

    assert!(result.phases.contains(&OptimizerPhase::Normalize));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.phase == OptimizerPhase::ConstantFolding
            && diagnostic.decision == OptimizerDecisionKind::ConstantValueFolded
            && diagnostic.source_operation_ordinal == Some(1)
            && diagnostic.resulting_operation_ordinal == Some(1)
    }));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.phase == OptimizerPhase::ConstantFolding
            && diagnostic.decision == OptimizerDecisionKind::ConstantValueFolded
            && diagnostic.source_operation_ordinal == Some(2)
            && diagnostic.resulting_operation_ordinal == Some(2)
    }));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.phase == OptimizerPhase::Normalize
            && diagnostic.decision == OptimizerDecisionKind::PredicateNormalized
            && diagnostic.source_operation_ordinal == Some(0)
            && diagnostic.resulting_operation_ordinal == Some(0)
    }));
}

#[test]
fn no_optimization_config_records_explicit_skipped_rewrite_passes() {
    let ir = procedure(vec![SrplBusinessOperationIr {
        ordinal: 0,
        kind: SrplBusinessOperationKindIr::Read {
            source: qn("Inventory.ProductStock"),
            binding: "Stock".to_string(),
            cardinality: Cardinality::One,
            predicates: vec![eq_pred("ProductId", "Stock", "ProductId")],
        },
    }]);

    let result = optimize_procedure_ir_with_config(
        ir,
        OptimizerPipelineConfig::new(OptimizationLevel::None).with_noop_decisions(true),
    )
    .expect("optimizer none mode should still emit evidence");

    let skipped: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.decision == OptimizerDecisionKind::PassSkipped)
        .map(|diagnostic| diagnostic.phase)
        .collect();

    assert_eq!(
        skipped,
        vec![
            OptimizerPhase::ConstantFolding,
            OptimizerPhase::PredicatePushdown,
            OptimizerPhase::Normalize,
        ]
    );
}
