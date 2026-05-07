//! HA/DR stream mapping over QUIC connections.
//!
//! This module defines the stream multiplexing strategy for WAL shipping, replica heartbeats,
//! and promotion votes over QUIC. It ensures deterministic stream ID allocation, proper flow
//! control via backpressure, and orphan stream cleanup on connection failure.
//!
//! ## Architecture
//!
//! QUIC streams are reserved and allocated as follows:
//!
//! - **Client-initiated streams (even IDs)**: Client sends RPC commands; server replies on result streams
//! - **Server-initiated streams (odd IDs)**: Server sends server-driven commands (heartbeats, votes)
//!
//! Andromeda partitions stream space into semantic ranges:
//!
//! ```text
//! ┌─ Stream ID Range ────────────────────────────────────────────┐
//! │                                                               │
//! │  [0-127]         [128-255]         [256-511]   [512-∞]      │
//! │  Application     HA/DR Control     (Reserved)  (Reserved)   │
//! │  RPC Streams     Heartbeat / Votes                          │
//! │  (Quorum, etc.)  WAL Shipping                               │
//! │                  Backpressure                               │
//! │                                                               │
//! └───────────────────────────────────────────────────────────────┘
//! ```
//!
//! The **HA/DR Control** range [128-255] is further subdivided:
//!
//! - **[128-159]** (32 stream IDs / 16 replica slots): Replica heartbeat channels (bidirectional)
//! - **[160-191]** (32 stream IDs / 16 replica slots): WAL shipping channels (unidirectional primary→replica)
//! - **[192-223]** (32 stream IDs / 16 replica slots): Promotion vote channels (bidirectional)
//! - **[224-255]** (32 streams): Reserved for future use (backpressure, control signals)
//!
//! ## Deterministic Allocation
//!
//! Stream IDs are allocated **deterministically** based on topology. Given a replica at index `r`:
//!
//! - **Heartbeat Stream ID**: `128 + (r * 2)` (bidirectional, opened by primary)
//! - **WAL Shipping Stream ID**: `160 + (r * 2)` (unidirectional, primary→replica)
//! - **Promotion Vote Stream ID**: `192 + (r * 2)` (bidirectional, opened by replica)
//!
//! This ensures that the same topology always produces the same stream allocation,
//! enabling deterministic replay and consistent failover decisions.
//!
//! ## Backpressure Integration
//!
//! The WAL shipping stream is backpressure-aware. When a replica falls behind:
//!
//! 1. Replica sends `ShippingBackpressureRequest` on the heartbeat stream
//! 2. Primary receives request and applies backpressure to WAL shipping
//! 3. Backpressure is reflected in QUIC flow control (stream credit consumption throttled)
//! 4. Client-side WAL sender blocks until replica catches up
//!
//! See D5 backpressure model for detailed flow control semantics.
//!
//! ## Orphan Stream Cleanup
//!
//! If a replica connection fails or times out:
//!
//! 1. Streams become "orphaned" (no active connection)
//! 2. `HadrStreamCleanup` identifies all orphaned stream IDs
//! 3. Stream credits are returned to the connection pool
//! 4. On reconnect, new streams are allocated with the same IDs
//!
//! Cleanup is deterministic and idempotent, enabling safe recovery.
//!
//! ## Integration Points
//!
//! - **F1 (WAL Shipping Runtime)**: Uses WAL shipping stream; respects backpressure
//! - **F3 (Quorum Runtime)**: Uses promotion vote stream for replica rankings
//! - **D4 (Procedure Gateway)**: Routes heartbeat frames from executor layer
//! - **D5 (Backpressure Model)**: Implements flow control and credit management

mod allocation;
mod cleanup;
mod kind;
mod multiplexer;

pub use allocation::StreamAllocation;
pub use cleanup::HadrStreamCleanup;
pub use kind::HadrStreamKind;
pub use multiplexer::StreamMultiplexer;

/// Reserved stream ID range for HA/DR control operations.
///
/// QUIC stream IDs are u64, but Andromeda reserves a canonical range for all
/// HA/DR traffic. This ensures deterministic allocation across restarts.
pub const HADR_STREAM_MIN: u64 = 128;
pub const HADR_STREAM_MAX: u64 = 255;

const HADR_STREAM_STRIDE: u64 = 2;
const HADR_STREAMS_PER_REPLICA: usize = 3;

/// Heartbeat stream subrange: [128-159].
pub const HEARTBEAT_STREAM_MIN: u64 = 128;
pub const HEARTBEAT_STREAM_MAX: u64 = 159;
pub const HEARTBEAT_MAX_REPLICAS: u64 =
    replica_slots_for_quic_range(HEARTBEAT_STREAM_MIN, HEARTBEAT_STREAM_MAX);

/// WAL shipping stream subrange: [160-191].
pub const WAL_SHIPPING_STREAM_MIN: u64 = 160;
pub const WAL_SHIPPING_STREAM_MAX: u64 = 191;
pub const WAL_SHIPPING_MAX_REPLICAS: u64 =
    replica_slots_for_quic_range(WAL_SHIPPING_STREAM_MIN, WAL_SHIPPING_STREAM_MAX);

/// Promotion vote stream subrange: [192-223].
pub const VOTE_STREAM_MIN: u64 = 192;
pub const VOTE_STREAM_MAX: u64 = 223;
pub const VOTE_MAX_REPLICAS: u64 = replica_slots_for_quic_range(VOTE_STREAM_MIN, VOTE_STREAM_MAX);

/// Reserved/future stream subrange: [224-255].
pub const RESERVED_STREAM_MIN: u64 = 224;
pub const RESERVED_STREAM_MAX: u64 = 255;

const fn replica_slots_for_quic_range(min: u64, max: u64) -> u64 {
    (max - min).div_ceil(HADR_STREAM_STRIDE)
}

#[cfg(test)]
mod tests;
