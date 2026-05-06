mod handler;
mod inventory_handlers;
mod registry_core;
mod validation;

pub use handler::ProcedureHandler;
pub use inventory_handlers::{
    InventoryQueryStockProcedureHandler, InventoryReleaseStockProcedureHandler,
    ReserveStockProcedureHandler,
};
pub use registry_core::ProcedureRegistry;

#[cfg(test)]
mod tests;
