#![forbid(unsafe_code)]

//! Runtime-free RPC orchestration contracts.
//!
//! This crate owns dispatch glue between frame protocol validation and typed
//! RPC payload envelope validation. It does not own QUIC sockets, Quinn, TLS,
//! executor dispatch, IAM policy, WAL, storage, or recovery behavior.

mod dispatch;

pub use dispatch::{
    DispatchPolicy, FrameDispatch, TransportSurface, dispatch_frame, expected_stream_role,
    validate_transport_surface,
};
