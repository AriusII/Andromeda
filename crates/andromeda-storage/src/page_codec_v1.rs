//! Compatibility facade for the page codec owner crate.
//!
//! Page codec V1 ownership now lives in `andromeda-storage-page`. This module
//! preserves historical `andromeda_storage::PageCodecV1` imports during the
//! crate extraction.

pub use andromeda_storage_page::{
    DecodedPageV1, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_TRAILER_LEN, PageCodecV1,
    integrity_trailer_for_payload, payload_crc64, payload_hash, torn_write_guard,
    validate_payload_integrity,
};
