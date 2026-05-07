//! Compatibility facade for runtime-free backpressure contracts.
//!
//! Lot 5.2B2 moved canonical backpressure signal ownership to
//! `andromeda-rpc-protocol`. `andromeda-quic` root reexports remain
//! available during migration so existing callers can move imports without
//! behavior changes.

pub use andromeda_rpc_protocol::backpressure::{
    BackpressureReason, BackpressureSignal, BackpressureTransport,
};
