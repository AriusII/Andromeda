//! Public SRPL compiler orchestration.
//!
//! Source-to-definition helpers live in `andromeda-srpl-definition-batch`; this
//! crate keeps the legacy public entry points and SRPL-root optimizer wrapper.

use andromeda_optimizer::srpl::{
    OptimizerPipelineConfig, OptimizerPipelineResult, optimize_procedure_ir_with_config,
    phase::OptimizerPhase,
};
use andromeda_srpl_diagnostics::{DiagnosticPhase, SrplDiagnostic};

pub use andromeda_srpl_definition_batch::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_inventory_reserve_stock_contract,
    compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
    inventory_reserve_stock_contract_metadata, lower_ir_to_catalog_definition,
};

/// Parses, binds, lowers, and optimizes a narrow SRPL procedure source.
///
/// Source diagnostics from parse/bind/lower phases are returned before the
/// optimizer is invoked, preserving their original source spans.
pub fn compile_narrow_procedure_signature_with_optimizer(
    source: &str,
    optimizer_config: OptimizerPipelineConfig,
) -> Result<OptimizerPipelineResult, SrplDiagnostic> {
    let ir = compile_narrow_procedure_signature(source)?;
    let procedure_name = ir.name.as_catalog_path();
    let mut result = optimize_procedure_ir_with_config(ir, optimizer_config).map_err(|error| {
        SrplDiagnostic::new(
            DiagnosticPhase::IrLowering,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })?;
    let mut phases = vec![
        OptimizerPhase::Parsing,
        OptimizerPhase::Binding,
        OptimizerPhase::IRLowering,
    ];
    phases.extend(std::mem::take(&mut result.phases));
    result.phases = phases;
    Ok(result)
}
