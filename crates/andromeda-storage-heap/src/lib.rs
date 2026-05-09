#![forbid(unsafe_code)]
#![doc = r#"
C5 owner crate for Andromeda heap storage boundaries.

This crate owns heap page layout, slot directory validation, heap page mutation,
raw heap scans, and vacuum planning. Storage remains the compatibility surface
for row-domain helpers and WAL redo envelope construction while those contracts
are promoted.

C5 invariants:

- Heap visibility must not outrun durable transaction commit evidence.
- Heap page flushes must not outrun durable WAL coverage through their page LSNs.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

mod compact;
mod delete;
mod error;
mod format_v1;
mod heap_redo;
mod insert;
mod local_redo_template;
mod page;
mod row_encoder;
mod scan;
mod slot;
mod tuple_layout;
mod vacuum;
mod validation;

pub mod slot_directory;

pub use format_v1::HEAP_PAGE_V1_PAYLOAD_OFFSET;
pub use heap_redo::{
    HEAP_ROW_REDO_HEADER_LEN, HEAP_ROW_REDO_NONE_SLOT_ID, HEAP_ROW_REDO_PAYLOAD_MAGIC,
    HEAP_ROW_REDO_PAYLOAD_VERSION, HeapRowRedoOperation, HeapRowRedoPayloadError,
    HeapRowRedoPayloadV1,
};
pub use insert::{HeapPageInsert, ProductStockHeapInsert};
pub use local_redo_template::{LocalHeapRowInsertRedoTemplate, LocalHeapRowRedoContractBinding};
pub use page::HeapPage;
pub use row_encoder::{
    ColumnDef, Datum, INVENTORY_PRODUCT_STOCK_TABLE_NAME, PRODUCT_STOCK_PRODUCT_ID_COLUMN,
    PRODUCT_STOCK_QUANTITY_ON_HAND_COLUMN, PRODUCT_STOCK_ROW_ENCODED_LEN, ProductStockRow,
    RowEncoder, RowSchema, ScalarType, product_stock_row_encoder, product_stock_row_schema,
};
pub use scan::{HeapScanIter, ProductStockHeapScanIter};
pub use slot::SlotEntry;
pub use slot_directory::{SlotDirectory, SlotId};
pub use vacuum::{HeapVacuumMode, HeapVacuumPlan, HeapVacuumReport};

pub(crate) use error::heap_error;
pub(crate) use format_v1::{
    HEAP_PAGE_V1_SLOT_FLAGS_KNOWN_MASK, HEAP_PAGE_V1_SLOT_METADATA_SIZE, HEAP_PAGE_V1_TRAILER_SIZE,
    HeapPageV1SlotMetadata, heap_page_v1_max_slots, heap_page_v1_metadata_offset,
    heap_page_v1_read_slot_metadata, heap_page_v1_validate_format_guard,
};
pub(crate) use validation::heap_page_v1_read_and_validate_slots;
