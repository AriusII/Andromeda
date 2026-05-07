//! Stream protocol layer: roles, flow control, and byte ordering.
//!
//! `StreamRole` and `FrameFamily` are re-exported through the frame module
//! for backward compatibility and consistency.

pub use crate::stream_types::{FrameFamily, StreamRole};
