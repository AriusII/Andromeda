//! QUIC stream protocol layer.
//!
//! This module defines stream roles, flow control, byte ordering guarantees, and state machines.

pub mod types;

pub use types::{FrameFamily, StreamRole};
