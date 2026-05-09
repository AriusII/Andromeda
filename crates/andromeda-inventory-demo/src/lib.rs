#![forbid(unsafe_code)]

mod business;
mod inventory_handlers;
mod vertical_slice_entry;

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
