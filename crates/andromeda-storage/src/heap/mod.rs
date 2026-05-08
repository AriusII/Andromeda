//! Heap page V1 layout and heap operations.
//!
//! Layout summary:
//! - fixed PageHeader region plus reserved durable-codec overlay bytes
//! - tuple payload region growing upward
//! - slot directory growing downward
//! - trailer + slot metadata

mod compact;
mod delete;
mod error;
mod format_v1;
mod insert;
mod page;
mod scan;
mod vacuum;
mod validation;

pub mod slot_directory;

pub use andromeda_storage_heap::SlotEntry;
pub use insert::{HeapPageInsert, ProductStockHeapInsert};
pub use page::HeapPage;
pub use scan::{HeapScanIter, ProductStockHeapScanIter};
pub use vacuum::{HeapVacuumMode, HeapVacuumPlan, HeapVacuumReport};

pub(crate) use error::heap_error;
pub(crate) use format_v1::{
    HEAP_PAGE_V1_SLOT_FLAGS_KNOWN_MASK, HEAP_PAGE_V1_SLOT_METADATA_SIZE, HEAP_PAGE_V1_TRAILER_SIZE,
    HeapPageV1SlotMetadata, heap_page_v1_max_slots, heap_page_v1_metadata_offset,
    heap_page_v1_read_slot_metadata, heap_page_v1_validate_format_guard,
};
pub(crate) use validation::heap_page_v1_read_and_validate_slots;

pub use format_v1::HEAP_PAGE_V1_PAYLOAD_OFFSET;
