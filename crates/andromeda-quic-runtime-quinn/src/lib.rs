#![forbid(unsafe_code)]

//! Quinn runtime adapter for Andromeda QUIC transport.
//!
//! Runtime-free stream, frame, Procedure route, reconnect, and 0-RTT policy
//! contracts stay in `andromeda-quic`, `andromeda-rpc`, and
//! `andromeda-rpc-protocol`. This crate owns concrete Quinn, Rustls, Tokio,
//! and runtime socket wiring.

mod runtime_quinn;

pub mod quinn_backend;
pub mod quinn_tls;
