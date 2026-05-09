//! Public lowering facade for the bounded SRPL compiler slice.
//!
//! `andromeda-srpl-lowering` owns bound AST to IR lowering. This module keeps
//! orchestration in this crate: binding adaptation, optimizer entry points,
//! and catalog contract materialization.

mod binding;
mod contract;

use andromeda_error::AndromedaResult;
use andromeda_optimizer::srpl::{
    OptimizerPipelineConfig, OptimizerPipelineResult, optimize_procedure_ir_with_config,
    phase::OptimizerPhase,
};
use andromeda_srpl_binder::{BoundProcedure, bind_procedure, validate_ast_names_for_diagnostics};
use andromeda_srpl_diagnostics::{
    DiagnosticPhase, SrplDiagnostic, SrplSource, enrich_source_diagnostic,
};
use andromeda_srpl_ir::SrplProcedureIr;
use andromeda_srpl_lowering::BoundProcedureLoweringInput;
use andromeda_srpl_parser::parse_procedure_signature;

pub use binding::{bind_executable_procedure_plan, inventory_reserve_stock_body_ir};
pub use contract::{
    compile_inventory_reserve_stock_contract, compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, inventory_reserve_stock_contract_metadata,
    lower_ir_to_catalog_definition,
};

/// Canonical SRPL source for `Inventory.ReserveStock`.
pub const INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE: &str = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;";

/// Lowers a bound procedure to [`SrplProcedureIr`] through the dedicated
/// lowering crate.
pub fn lower_bound_procedure(bound: BoundProcedure) -> AndromedaResult<SrplProcedureIr> {
    andromeda_srpl_lowering::lower_bound_procedure(BoundProcedureLoweringInput {
        signature: bound.signature,
        body: bound.body,
    })
}

/// Parses, binds, and lowers a narrow SRPL procedure source to [`SrplProcedureIr`].
///
/// Returns a [`SrplDiagnostic`] on the first detected violation.
pub fn compile_narrow_procedure_signature(source: &str) -> Result<SrplProcedureIr, SrplDiagnostic> {
    let srpl_source = SrplSource::new(source);
    if let Some(diagnostic) = srpl_source
        .forbidden_construct_diagnostics()
        .into_iter()
        .next()
    {
        return Err(enrich_source_diagnostic(source, diagnostic, None));
    }

    let ast = parse_procedure_signature(source)
        .map_err(|diagnostic| enrich_source_diagnostic(source, diagnostic, None))?;
    let procedure_name = ast.name.value.as_catalog_path();
    validate_ast_names_for_diagnostics(&ast, source)?;
    let bound = bind_procedure(ast).map_err(|error| {
        SrplDiagnostic::new(
            DiagnosticPhase::Binding,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })?;
    let procedure_name = bound.signature.name.as_catalog_path();
    lower_bound_procedure(bound).map_err(|error| {
        SrplDiagnostic::new(
            DiagnosticPhase::IrLowering,
            None,
            format!("{}; procedure {}", error, procedure_name),
        )
    })
}

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
