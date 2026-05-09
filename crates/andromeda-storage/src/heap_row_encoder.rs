//! Compatibility facade for heap row encoding.
//!
//! Row schemas, datum encoding, and the `Inventory.ProductStock` row shape now
//! live in `andromeda-storage-heap`.

pub use andromeda_storage_heap::{
    ColumnDef, Datum, INVENTORY_PRODUCT_STOCK_TABLE_NAME, PRODUCT_STOCK_PRODUCT_ID_COLUMN,
    PRODUCT_STOCK_QUANTITY_ON_HAND_COLUMN, PRODUCT_STOCK_ROW_ENCODED_LEN, ProductStockRow,
    RowEncoder, RowSchema, ScalarType, product_stock_row_encoder, product_stock_row_schema,
};
