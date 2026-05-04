//! RPC dispatch layer.
//!
//! This module handles stream-to-RPC mapping and RPC dispatch.

pub mod dispatch;

pub use dispatch::{
    dispatch_frame, expected_stream_role, validate_transport_surface, DispatchPolicy,
    FrameDispatch, TransportSurface,
};
