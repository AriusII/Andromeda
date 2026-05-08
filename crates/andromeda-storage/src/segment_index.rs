//! Compatibility facade for the segment index owner crate.
//!
//! Segment-index codecs and contracts are owned by `andromeda-segment` during
//! the storage extraction. Storage keeps this module as a reexport so existing
//! callers can migrate without changing durable byte semantics.

pub use andromeda_segment::segment_index::*;
