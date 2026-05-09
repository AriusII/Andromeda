//! HA/DR logical stream mapping over QUIC connections.
//!
//! This module defines the stream multiplexing strategy for WAL shipping,
//! replica heartbeats, and promotion votes before transport binding. It ensures
//! deterministic logical stream allocation, proper flow control via
//! backpressure, and orphan stream cleanup on connection failure.
//!
//! ## Architecture
//!
//! Andromeda partitions a logical HA/DR stream space into semantic ranges:
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
//! - **[128-159]**: Replica heartbeat channels.
//! - **[160-191]**: WAL shipping channels.
//! - **[192-223]**: Promotion vote channels.
//! - **[224-255]**: Reserved for future use.
//!
//! ## Deterministic Allocation
//!
//! Logical stream IDs are allocated deterministically from topology. Given a
//! replica at index `r`:
//!
//! - **Heartbeat logical stream ID**: `128 + r`
//! - **WAL shipping logical stream ID**: `160 + r`
//! - **Promotion vote logical stream ID**: `192 + r`
//!
//! This ensures that the same topology always produces the same stream allocation,
//! enabling deterministic replay and consistent failover decisions.
//!
//! These values are not wire-level QUIC stream IDs. QUIC stream IDs encode
//! initiator and directionality in low bits and are allocated by the transport.
//! A transport binding maps each HADR logical stream ID to whatever QUIC stream
//! ID was actually opened.
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

/// Reserved logical stream ID range for HA/DR control operations.
///
/// The transport layer maps these logical IDs to concrete QUIC stream IDs.
/// Keeping this model logical avoids coupling HADR decisions to QUIC low-bit
/// initiator and directionality encoding.
pub const HADR_STREAM_MIN: u64 = 128;
pub const HADR_STREAM_MAX: u64 = 255;

const HADR_STREAM_STRIDE: u64 = 1;
const HADR_STREAMS_PER_REPLICA: usize = 3;

/// Heartbeat stream subrange: [128-159].
pub const HEARTBEAT_STREAM_MIN: u64 = 128;
pub const HEARTBEAT_STREAM_MAX: u64 = 159;
pub const HEARTBEAT_MAX_REPLICAS: u64 =
    replica_slots_for_logical_range(HEARTBEAT_STREAM_MIN, HEARTBEAT_STREAM_MAX);

/// WAL shipping stream subrange: [160-191].
pub const WAL_SHIPPING_STREAM_MIN: u64 = 160;
pub const WAL_SHIPPING_STREAM_MAX: u64 = 191;
pub const WAL_SHIPPING_MAX_REPLICAS: u64 =
    replica_slots_for_logical_range(WAL_SHIPPING_STREAM_MIN, WAL_SHIPPING_STREAM_MAX);

/// Promotion vote stream subrange: [192-223].
pub const VOTE_STREAM_MIN: u64 = 192;
pub const VOTE_STREAM_MAX: u64 = 223;
pub const VOTE_MAX_REPLICAS: u64 =
    replica_slots_for_logical_range(VOTE_STREAM_MIN, VOTE_STREAM_MAX);

/// Reserved/future stream subrange: [224-255].
pub const RESERVED_STREAM_MIN: u64 = 224;
pub const RESERVED_STREAM_MAX: u64 = 255;

/// Deterministic HADR stream identity before binding to a transport stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HadrLogicalStreamId(u64);

impl HadrLogicalStreamId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_hadr_reserved(self) -> bool {
        self.0 >= HADR_STREAM_MIN && self.0 <= HADR_STREAM_MAX
    }
}

impl From<HadrLogicalStreamId> for u64 {
    fn from(value: HadrLogicalStreamId) -> Self {
        value.get()
    }
}

const fn replica_slots_for_logical_range(min: u64, max: u64) -> u64 {
    max - min + 1
}

#[cfg(test)]
mod tests;
