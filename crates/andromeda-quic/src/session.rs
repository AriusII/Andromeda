//! Connection protocol layer: session lifecycle and surface-plane gating.
//!
//! Re-exports the canonical `crate::connection` state machine. The module is
//! named `session` to avoid colliding with the private implementation file
//! while keeping the public concept of a QUIC session discoverable.

pub use crate::connection::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
};
