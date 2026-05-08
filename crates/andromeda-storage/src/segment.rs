//! Compatibility re-exports for durable segment ownership.
//!
//! Segment identities, descriptors, and mutation gates now live in
//! `andromeda-segment`.

pub use andromeda_segment::{
    SegmentDescriptor, SegmentDurabilityBoundary, SegmentHeader, SegmentId, SegmentMutation,
    SegmentState, SegmentTrailer,
};
