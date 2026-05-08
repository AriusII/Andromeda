//! RPC dispatch layer: stream-to-RPC mapping and dispatch.

pub use andromeda_rpc::{
    DispatchPolicy, FrameDispatch, TransportSurface, dispatch_frame, expected_stream_role,
    validate_transport_surface,
};
