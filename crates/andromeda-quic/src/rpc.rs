//! RPC dispatch layer: stream-to-RPC mapping and dispatch.

pub use crate::rpc_dispatch::{
    dispatch_frame, expected_stream_role, validate_transport_surface, DispatchPolicy,
    FrameDispatch, TransportSurface,
};
