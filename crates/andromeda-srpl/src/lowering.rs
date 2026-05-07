//! Public lowering facade for the bounded SRPL compiler slice.

mod binding;
mod contract;
mod diagnostics;
mod pipeline;
mod validation;

pub use binding::{bind_executable_procedure_plan, inventory_reserve_stock_body_ir};
pub use contract::{
    compile_inventory_reserve_stock_contract, compile_inventory_reserve_stock_contract_candidate,
    compile_narrow_procedure_contract_candidate, compile_narrow_procedure_definition,
    compile_narrow_procedure_definition_batch, inventory_reserve_stock_contract_metadata,
    lower_ir_to_catalog_definition, lower_ir_to_contract_candidate,
};
pub use pipeline::{
    INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, compile_narrow_procedure_signature,
    compile_narrow_procedure_signature_with_optimizer, lower_body_ast, lower_bound_procedure,
};
