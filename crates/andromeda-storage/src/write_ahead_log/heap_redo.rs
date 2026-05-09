//! Compatibility reexports for versioned heap row redo WAL payloads.
//!
//! Canonical heap redo payload ownership now lives in `andromeda_storage_heap`.

pub use andromeda_storage_heap::{
    HEAP_ROW_REDO_HEADER_LEN, HEAP_ROW_REDO_NONE_SLOT_ID, HEAP_ROW_REDO_PAYLOAD_MAGIC,
    HEAP_ROW_REDO_PAYLOAD_VERSION, HeapRowRedoOperation, HeapRowRedoPayloadError,
    HeapRowRedoPayloadV1,
};
