/// Result stream id for `Inventory.ReserveStock`.
pub const INVENTORY_RESERVE_STOCK_RESULT_STREAM_ID: u64 = 1;

/// Exact result row count guaranteed by `Inventory.ReserveStock`.
pub const INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS: u64 = 1;

/// Number of stock rows affected by a successful `Inventory.ReserveStock` invocation.
pub const INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED: u64 = 1;

/// Number of reservation rows affected by a successful `Inventory.ReserveStock` invocation.
pub const INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED: u64 = 1;

/// Domain tag embedded in the mutation payload for `Inventory.ReserveStock`.
pub(super) const RESERVE_STOCK_PAYLOAD_DOMAIN: &[u8] =
    b"andromeda.business.inventory.reserve-stock.v1";
