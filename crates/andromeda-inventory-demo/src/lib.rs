#![forbid(unsafe_code)]

mod business;
mod inventory_handlers;
mod vertical_slice_entry;

pub use andromeda_business_fixtures::inventory_catalog::{
    INVENTORY_DATABASE_ID, INVENTORY_DEFINITION_BATCH_ID, INVENTORY_NAMESPACE_ID,
    INVENTORY_PRODUCT_STOCK_OBJECT_ID, INVENTORY_QUERY_STOCK_OBJECT_ID,
    INVENTORY_QUERY_STOCK_PERMISSION, INVENTORY_QUERY_STOCK_PROCEDURE_ID,
    INVENTORY_RELEASE_STOCK_OBJECT_ID, INVENTORY_RELEASE_STOCK_PERMISSION,
    INVENTORY_RELEASE_STOCK_PROCEDURE_ID, INVENTORY_RESERVATION_OBJECT_ID,
    INVENTORY_RESERVE_STOCK_OBJECT_ID, INVENTORY_RESERVE_STOCK_PERMISSION,
    INVENTORY_RESERVE_STOCK_PROCEDURE_ID, InventoryReserveStockCatalogBindings,
    inventory_domain_definition_batch, inventory_product_stock_table,
    inventory_protocol_layout_ref, inventory_query_stock_contract,
    inventory_query_stock_contract_candidate, inventory_release_stock_contract,
    inventory_release_stock_contract_candidate, inventory_reservation_structured_object,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
    inventory_reserve_stock_contract_candidate,
};
pub use business::{
    HeapInventoryProductStockStore, INVENTORY_QUERY_STOCK_COLUMN_COUNT,
    INVENTORY_QUERY_STOCK_RESULT_STREAM_ID, INVENTORY_RELEASE_STOCK_EXACT_RESULT_ROWS,
    INVENTORY_RELEASE_STOCK_RESERVATION_ROWS_FREED, INVENTORY_RELEASE_STOCK_RESULT_STREAM_ID,
    INVENTORY_RELEASE_STOCK_STOCK_ROWS_AFFECTED, INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS,
    INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED, INVENTORY_RESERVE_STOCK_RESULT_STREAM_ID,
    INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED, InventoryBusinessMvccStore,
    InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence,
    InventoryProductStockReservationIntent, InventoryProductStockStore, InventoryReservation,
    InventoryReserveStockExecutor, InventoryReserveStockMvccDecision,
    InventoryReserveStockMvccEvidence, InventoryReserveStockRejectionEvidence,
    InventoryReserveStockResultEvidence, InventoryStock, InventoryStockVersionEvidence,
    ObservedInventoryProductStockStore, QueryStockCommand, QueryStockEffect, ReleaseStockCommand,
    ReleaseStockEffect, ReservationResult, ReserveStockCommand, ReserveStockEffect,
};
pub use inventory_handlers::{
    InventoryQueryStockProcedureHandler, InventoryReleaseStockProcedureHandler,
    ReserveStockProcedureHandler,
};
pub use vertical_slice_entry::{
    V0InventoryProtocolViolation, V0InventoryRecoverableOutcome, V0InventoryRecoverableRuntime,
    V0InventoryReserveStockExecutableProcedure, V0InventoryReserveStockRpcPayload,
    bind_inventory_reserve_stock_v0_pdf_executable_procedure, decode_v0_execute_frame,
    emit_v0_inventory_reserve_stock_pre_transaction_refusal,
    encode_inventory_reserve_stock_v0_execute_frame,
};
