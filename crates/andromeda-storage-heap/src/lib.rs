#![forbid(unsafe_code)]
#![doc = r#"
C5 owner crate for Andromeda heap storage boundaries.

This crate owns heap page layout, slot directory validation, heap page mutation,
raw heap scans, and vacuum planning. Storage remains the compatibility facade
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
mod insert;
mod page;
mod scan;
mod slot;
mod vacuum;
mod validation;

pub mod slot_directory;

pub use format_v1::HEAP_PAGE_V1_PAYLOAD_OFFSET;
pub use insert::HeapPageInsert;
pub use page::HeapPage;
pub use scan::HeapScanIter;
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
