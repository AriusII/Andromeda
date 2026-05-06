//! Connection lifecycle and surface plane gating.
//!
//! Implements the V0 QUIC session state machine:
//!
//! ```text
//! new() -> Hello -> Auth -> Active -> Draining -> Closed
//!                         \-----------------------> Closed
//! ```
//!
//! The state machine is intentionally non-network-aware: it operates on parsed
//! `FrameBytes` and produces typed protocol errors. Wiring to a real QUIC
//! socket layer is performed elsewhere; this module is the contract surface.
//!
//! Surface planes partition session intent. A connection is bound to exactly
//! one plane at construction. RPC dispatch declares the operational class of
//! the request; cross-plane requests (e.g. an Administration command on an
//! Application connection) are rejected with `AndromedaErrorKind::Protocol`.

mod cancellation;
mod error;
mod lifecycle;
mod listener;
mod state_machine;
mod surface;

pub use cancellation::{CancellationCause, CancellationOutcome, CancellationSignal};
pub use lifecycle::LifecycleState;
pub use listener::{DatagramPolicy, EarlyDataPolicy, SurfaceListenerConfig, SurfaceListenerSet};
pub use state_machine::Connection;
pub use surface::SurfacePlane;

#[cfg(test)]
mod tests;
