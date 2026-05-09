#![forbid(unsafe_code)]

//! SRPL compiler orchestration.
//!
//! This crate keeps source-to-contract orchestration around the SRPL owner
//! crates. Parser, binder, diagnostics, lowering, optimizer, execution-adapter,
//! interpreter, AST, cardinality, and IR types are imported from their owner
//! crates directly.

mod lowering;

pub use lowering::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_inventory_reserve_stock_contract,
    compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, compile_narrow_procedure_signature,
    compile_narrow_procedure_signature_with_optimizer, inventory_reserve_stock_contract_metadata,
    lower_ir_to_catalog_definition,
};
