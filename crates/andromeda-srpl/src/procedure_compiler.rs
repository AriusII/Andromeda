//! Compiler-stage API for SRPL.
//!
//! This module groups the narrow SRPL compiler pipeline by phase. Root-level
//! re-exports remain available for compatibility, but new code should prefer
//! importing lexer/parser/binder/lowering entry points from this boundary.

pub use crate::binder::{BoundProcedure, bind_procedure};
pub use crate::definition_batch_bridge::{
    SrplDefinitionBatchDiagnostic, SrplDefinitionBatchDryRunError, SrplDefinitionBatchDryRunReport,
    SrplDefinitionBatchDryRunRequest, SrplDefinitionBatchDurableApplyReport,
    SrplDefinitionBatchProcedureSource, SrplDefinitionBatchSourceEvidence,
    SrplProcedureDryRunManifest, SrplProcedureSourceDigest, SrplProcedureSourceDigestEvidence,
    dry_run_srpl_definition_batch_sources,
};
pub use crate::lexer::{Token, TokenKind, lex};
pub use crate::lowering::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, bind_executable_procedure_plan,
    compile_inventory_reserve_stock_contract, compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
    compile_narrow_procedure_signature_with_optimizer, inventory_reserve_stock_body_ir,
    inventory_reserve_stock_contract_metadata, lower_body_ast, lower_bound_procedure,
    lower_ir_to_catalog_definition, lower_ir_to_contract_candidate,
};
pub use crate::parser::parse_procedure_signature;
