#![forbid(unsafe_code)]

use andromeda_optimizer::srpl::{
    OptimizationLevel, OptimizerPipelineConfig, phase::OptimizerPhase,
};
use andromeda_srpl::compile_narrow_procedure_signature_with_optimizer;
use andromeda_srpl_diagnostics::DiagnosticPhase;

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
