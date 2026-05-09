//! Compiler-stage API for SRPL.
//!
//! This module exposes orchestration entry points that parse, bind, lower,
//! optimize, and materialize SRPL sources. Lexer, parser, binder, optimizer,
//! AST, and IR primitives are imported from their owner crates directly.

pub use crate::definition_batch_bridge::{
    SrplDefinitionBatchDiagnostic, SrplDefinitionBatchDryRunError, SrplDefinitionBatchDryRunReport,
    SrplDefinitionBatchDryRunRequest, SrplDefinitionBatchDurableApplyReport,
    SrplDefinitionBatchProcedureSource, SrplDefinitionBatchSourceEvidence,
    SrplProcedureDryRunManifest, SrplProcedureSourceDigest, SrplProcedureSourceDigestEvidence,
    dry_run_srpl_definition_batch_sources,
};
pub use crate::lowering::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, bind_executable_procedure_plan,
    compile_inventory_reserve_stock_contract, compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
    compile_narrow_procedure_signature_with_optimizer, inventory_reserve_stock_body_ir,
    inventory_reserve_stock_contract_metadata, lower_bound_procedure,
    lower_ir_to_catalog_definition,
};
