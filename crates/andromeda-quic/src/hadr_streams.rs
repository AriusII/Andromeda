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
//! - **[128-159]** (32 streams): Replica heartbeat channels (one per replica, bidirectional)
//! - **[160-191]** (32 streams): WAL shipping channels (one per replica, unidirectional primary→replica)
//! - **[192-223]** (32 streams): Promotion vote channels (one per replica, bidirectional)
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

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::{HashMap, HashSet};

/// Reserved stream ID range for HA/DR control operations.
///
/// QUIC stream IDs are u64, but Andromeda reserves a canonical range for all
/// HA/DR traffic. This ensures deterministic allocation across restarts.
pub const HADR_STREAM_MIN: u64 = 128;
pub const HADR_STREAM_MAX: u64 = 255;

/// Heartbeat stream subrange: [128-159].
pub const HEARTBEAT_STREAM_MIN: u64 = 128;
pub const HEARTBEAT_STREAM_MAX: u64 = 159;
pub const HEARTBEAT_MAX_REPLICAS: u64 = (HEARTBEAT_STREAM_MAX - HEARTBEAT_STREAM_MIN).div_ceil(2);

/// WAL shipping stream subrange: [160-191].
pub const WAL_SHIPPING_STREAM_MIN: u64 = 160;
pub const WAL_SHIPPING_STREAM_MAX: u64 = 191;
pub const WAL_SHIPPING_MAX_REPLICAS: u64 =
    (WAL_SHIPPING_STREAM_MAX - WAL_SHIPPING_STREAM_MIN).div_ceil(2);

/// Promotion vote stream subrange: [192-223].
pub const VOTE_STREAM_MIN: u64 = 192;
pub const VOTE_STREAM_MAX: u64 = 223;
pub const VOTE_MAX_REPLICAS: u64 = (VOTE_STREAM_MAX - VOTE_STREAM_MIN).div_ceil(2);

/// Reserved/future stream subrange: [224-255].
pub const RESERVED_STREAM_MIN: u64 = 224;
pub const RESERVED_STREAM_MAX: u64 = 255;

/// Kind of HA/DR stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HadrStreamKind {
    /// Bidirectional heartbeat exchange with replica (LSN position, health check).
    Heartbeat,
    /// Unidirectional WAL segment shipping from primary to replica.
    WalShipping,
    /// Bidirectional promotion vote aggregation.
    PromotionVote,
    /// Reserved for future use (backpressure, control signals).
    Control,
}

impl HadrStreamKind {
    /// Returns the minimum stream ID for this kind.
    pub const fn min_stream_id(self) -> u64 {
        match self {
            Self::Heartbeat => HEARTBEAT_STREAM_MIN,
            Self::WalShipping => WAL_SHIPPING_STREAM_MIN,
            Self::PromotionVote => VOTE_STREAM_MIN,
            Self::Control => RESERVED_STREAM_MIN,
        }
    }

    /// Returns the maximum stream ID for this kind.
    pub const fn max_stream_id(self) -> u64 {
        match self {
            Self::Heartbeat => HEARTBEAT_STREAM_MAX,
            Self::WalShipping => WAL_SHIPPING_STREAM_MAX,
            Self::PromotionVote => VOTE_STREAM_MAX,
            Self::Control => RESERVED_STREAM_MAX,
        }
    }

    /// Returns the maximum number of replicas supported by this stream kind.
    pub const fn max_replicas(self) -> u64 {
        match self {
            Self::Heartbeat => HEARTBEAT_MAX_REPLICAS,
            Self::WalShipping => WAL_SHIPPING_MAX_REPLICAS,
            Self::PromotionVote => VOTE_MAX_REPLICAS,
            Self::Control => 32,
        }
    }
}

/// Stream allocation information.
///
/// This struct holds deterministically allocated stream IDs for a replica
/// and provides query methods for stream kind lookup.
#[derive(Debug, Clone)]
pub struct StreamAllocation {
    /// Replica index in the topology (0-based).
    replica_index: u64,

    /// Heartbeat stream ID (bidirectional, opened by primary).
    heartbeat_stream_id: u64,

    /// WAL shipping stream ID (unidirectional, primary→replica).
    wal_shipping_stream_id: u64,

    /// Promotion vote stream ID (bidirectional, opened by replica).
    promotion_vote_stream_id: u64,
}

impl StreamAllocation {
    /// Creates a new stream allocation for a replica at the given index.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the replica index exceeds the maximum supported replicas.
    pub fn new(replica_index: u64) -> AndromedaResult<Self> {
        // Validate replica index is within bounds for all stream kinds
        if replica_index >= HEARTBEAT_MAX_REPLICAS
            || replica_index >= WAL_SHIPPING_MAX_REPLICAS
            || replica_index >= VOTE_MAX_REPLICAS
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "replica index {} exceeds maximum replicas supported",
                    replica_index
                ),
            ));
        }

        // Deterministic allocation: stream_id = base + (replica_index * 2)
        // (The factor of 2 accounts for QUIC's bidirectional vs. unidirectional distinction)
        let heartbeat_stream_id = HEARTBEAT_STREAM_MIN + (replica_index * 2);
        let wal_shipping_stream_id = WAL_SHIPPING_STREAM_MIN + (replica_index * 2);
        let promotion_vote_stream_id = VOTE_STREAM_MIN + (replica_index * 2);

        Ok(Self {
            replica_index,
            heartbeat_stream_id,
            wal_shipping_stream_id,
            promotion_vote_stream_id,
        })
    }

    /// Returns the replica index.
    pub const fn replica_index(&self) -> u64 {
        self.replica_index
    }

    /// Returns the heartbeat stream ID for this replica.
    pub const fn heartbeat_stream_id(&self) -> u64 {
        self.heartbeat_stream_id
    }

    /// Returns the WAL shipping stream ID for this replica.
    pub const fn wal_shipping_stream_id(&self) -> u64 {
        self.wal_shipping_stream_id
    }

    /// Returns the promotion vote stream ID for this replica.
    pub const fn promotion_vote_stream_id(&self) -> u64 {
        self.promotion_vote_stream_id
    }

    /// Looks up the stream kind for a given stream ID.
    ///
    /// Returns the kind if the stream ID is allocated to this replica;
    /// returns `None` if the stream ID does not belong to this allocation.
    pub fn stream_kind_for_id(&self, stream_id: u64) -> Option<HadrStreamKind> {
        match stream_id {
            id if id == self.heartbeat_stream_id => Some(HadrStreamKind::Heartbeat),
            id if id == self.wal_shipping_stream_id => Some(HadrStreamKind::WalShipping),
            id if id == self.promotion_vote_stream_id => Some(HadrStreamKind::PromotionVote),
            _ => None,
        }
    }
}

/// Stream multiplexer for HA/DR frames.
///
/// Routes frames from a QUIC connection to the appropriate HA/DR handler
/// (heartbeat, WAL shipping, promotion votes) based on stream ID.
///
/// Invariants:
/// - Stream IDs must be deterministically allocated (via `StreamAllocation`)
/// - No stream ID conflicts across different replicas
/// - Control stream (heartbeat, votes) have priority over data streams (WAL)
#[derive(Debug)]
pub struct StreamMultiplexer {
    /// Map of stream ID to allocated stream information.
    stream_allocations: HashMap<u64, (u64, HadrStreamKind)>, // stream_id -> (replica_index, kind)

    /// Set of open (non-terminal) stream IDs.
    open_streams: HashSet<u64>,

    /// Set of closed (terminal) stream IDs.
    closed_streams: HashSet<u64>,

    /// Maximum concurrent streams allowed (to prevent exhaustion).
    max_concurrent_streams: usize,
}

impl StreamMultiplexer {
    /// Creates a new stream multiplexer with default limits.
    pub fn new() -> Self {
        Self {
            stream_allocations: HashMap::new(),
            open_streams: HashSet::new(),
            closed_streams: HashSet::new(),
            max_concurrent_streams: 128,
        }
    }

    /// Creates a new stream multiplexer with a custom concurrency limit.
    pub fn with_max_concurrent(max_concurrent: usize) -> Self {
        Self {
            stream_allocations: HashMap::new(),
            open_streams: HashSet::new(),
            closed_streams: HashSet::new(),
            max_concurrent_streams: max_concurrent,
        }
    }

    /// Allocates streams for a new replica.
    ///
    /// This operation is deterministic: calling it multiple times with the same
    /// replica index will produce the same stream IDs.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - The replica index exceeds maximum replicas
    /// - The replica is already allocated
    /// - Stream allocation would exceed concurrency limits
    pub fn allocate_replica_streams(
        &mut self,
        replica_index: u64,
    ) -> AndromedaResult<StreamAllocation> {
        let allocation = StreamAllocation::new(replica_index)?;

        // Check for duplicate allocation
        if self
            .stream_allocations
            .contains_key(&allocation.heartbeat_stream_id)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!("replica {} already has allocated streams", replica_index),
            ));
        }

        // Check concurrency limits
        if self.open_streams.len() + 3 > self.max_concurrent_streams {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "allocating new replica streams would exceed concurrency limit",
            ));
        }

        // Record allocations
        self.stream_allocations.insert(
            allocation.heartbeat_stream_id,
            (replica_index, HadrStreamKind::Heartbeat),
        );
        self.stream_allocations.insert(
            allocation.wal_shipping_stream_id,
            (replica_index, HadrStreamKind::WalShipping),
        );
        self.stream_allocations.insert(
            allocation.promotion_vote_stream_id,
            (replica_index, HadrStreamKind::PromotionVote),
        );

        // Mark streams as open
        self.open_streams.insert(allocation.heartbeat_stream_id);
        self.open_streams.insert(allocation.wal_shipping_stream_id);
        self.open_streams
            .insert(allocation.promotion_vote_stream_id);

        Ok(allocation)
    }

    /// Looks up the stream kind and replica index for a given stream ID.
    ///
    /// Returns `(replica_index, stream_kind)` if the stream is allocated;
    /// returns `None` otherwise.
    pub fn lookup_stream(&self, stream_id: u64) -> Option<(u64, HadrStreamKind)> {
        self.stream_allocations.get(&stream_id).copied()
    }

    /// Marks a stream as closed (terminal).
    ///
    /// Once closed, the stream ID can be reused after terminal state is drained.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the stream ID is not allocated.
    pub fn close_stream(&mut self, stream_id: u64) -> AndromedaResult<()> {
        if !self.stream_allocations.contains_key(&stream_id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!("stream ID {} is not allocated", stream_id),
            ));
        }

        self.open_streams.remove(&stream_id);
        self.closed_streams.insert(stream_id);
        Ok(())
    }

    /// Returns the count of open streams.
    pub fn open_stream_count(&self) -> usize {
        self.open_streams.len()
    }

    /// Returns the count of closed streams.
    pub fn closed_stream_count(&self) -> usize {
        self.closed_streams.len()
    }

    /// Returns true if a stream ID is open.
    pub fn is_stream_open(&self, stream_id: u64) -> bool {
        self.open_streams.contains(&stream_id)
    }

    /// Returns true if a stream ID is closed.
    pub fn is_stream_closed(&self, stream_id: u64) -> bool {
        self.closed_streams.contains(&stream_id)
    }
}

impl Default for StreamMultiplexer {
    fn default() -> Self {
        Self::new()
    }
}

/// Orphan stream cleanup handler.
///
/// Detects and cleans up streams that become orphaned due to connection loss,
/// timeout, or explicit disconnection. Cleanup is deterministic and idempotent.
#[derive(Debug)]
pub struct HadrStreamCleanup {
    /// Stream IDs to be cleaned up.
    orphan_streams: HashSet<u64>,

    /// Stream IDs successfully cleaned (freed).
    cleaned_streams: HashSet<u64>,
}

impl HadrStreamCleanup {
    /// Creates a new cleanup handler.
    pub fn new() -> Self {
        Self {
            orphan_streams: HashSet::new(),
            cleaned_streams: HashSet::new(),
        }
    }

    /// Marks a stream as orphaned.
    ///
    /// The stream will be cleaned up (freed) when `execute_cleanup()` is called.
    pub fn mark_orphan(&mut self, stream_id: u64) {
        self.orphan_streams.insert(stream_id);
    }

    /// Marks multiple streams as orphaned (e.g., all streams for a replica).
    pub fn mark_orphans(&mut self, stream_ids: &[u64]) {
        for &stream_id in stream_ids {
            self.orphan_streams.insert(stream_id);
        }
    }

    /// Executes cleanup for all orphaned streams.
    ///
    /// This operation is idempotent: calling it multiple times is safe.
    /// Returns the set of successfully cleaned stream IDs.
    pub fn execute_cleanup(&mut self) -> HashSet<u64> {
        let cleaned = self.orphan_streams.drain().collect::<HashSet<_>>();
        self.cleaned_streams.extend(cleaned.iter());
        cleaned
    }

    /// Returns the count of orphaned (pending cleanup) streams.
    pub fn orphan_count(&self) -> usize {
        self.orphan_streams.len()
    }

    /// Returns the count of successfully cleaned streams.
    pub fn cleaned_count(&self) -> usize {
        self.cleaned_streams.len()
    }

    /// Returns true if there are pending orphan streams.
    pub fn has_orphans(&self) -> bool {
        !self.orphan_streams.is_empty()
    }
}

impl Default for HadrStreamCleanup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_allocation_deterministic() {
        let alloc1 = StreamAllocation::new(0).unwrap();
        let alloc2 = StreamAllocation::new(0).unwrap();

        assert_eq!(alloc1.heartbeat_stream_id(), alloc2.heartbeat_stream_id());
        assert_eq!(
            alloc1.wal_shipping_stream_id(),
            alloc2.wal_shipping_stream_id()
        );
        assert_eq!(
            alloc1.promotion_vote_stream_id(),
            alloc2.promotion_vote_stream_id()
        );
    }

    #[test]
    fn test_stream_allocation_non_overlapping() {
        let alloc0 = StreamAllocation::new(0).unwrap();
        let alloc1 = StreamAllocation::new(1).unwrap();

        // All stream IDs for different replicas must be distinct
        let ids0 = vec![
            alloc0.heartbeat_stream_id(),
            alloc0.wal_shipping_stream_id(),
            alloc0.promotion_vote_stream_id(),
        ];
        let ids1 = vec![
            alloc1.heartbeat_stream_id(),
            alloc1.wal_shipping_stream_id(),
            alloc1.promotion_vote_stream_id(),
        ];

        for &id0 in &ids0 {
            for &id1 in &ids1 {
                assert_ne!(id0, id1, "stream IDs must not overlap");
            }
        }
    }

    #[test]
    fn test_stream_allocation_bounds() {
        let alloc = StreamAllocation::new(0).unwrap();

        assert!(alloc.heartbeat_stream_id() >= HEARTBEAT_STREAM_MIN);
        assert!(alloc.heartbeat_stream_id() <= HEARTBEAT_STREAM_MAX);

        assert!(alloc.wal_shipping_stream_id() >= WAL_SHIPPING_STREAM_MIN);
        assert!(alloc.wal_shipping_stream_id() <= WAL_SHIPPING_STREAM_MAX);

        assert!(alloc.promotion_vote_stream_id() >= VOTE_STREAM_MIN);
        assert!(alloc.promotion_vote_stream_id() <= VOTE_STREAM_MAX);
    }

    #[test]
    fn test_stream_allocation_replica_index_bounds() {
        // Valid indices should succeed
        for i in 0..HEARTBEAT_MAX_REPLICAS
            .min(WAL_SHIPPING_MAX_REPLICAS)
            .min(VOTE_MAX_REPLICAS)
        {
            assert!(StreamAllocation::new(i).is_ok());
        }

        // Out-of-bounds index should fail
        let max_index = HEARTBEAT_MAX_REPLICAS
            .min(WAL_SHIPPING_MAX_REPLICAS)
            .min(VOTE_MAX_REPLICAS);
        assert!(StreamAllocation::new(max_index).is_err());
    }

    #[test]
    fn test_stream_kind_lookup() {
        let alloc = StreamAllocation::new(2).unwrap();

        assert_eq!(
            alloc.stream_kind_for_id(alloc.heartbeat_stream_id()),
            Some(HadrStreamKind::Heartbeat)
        );
        assert_eq!(
            alloc.stream_kind_for_id(alloc.wal_shipping_stream_id()),
            Some(HadrStreamKind::WalShipping)
        );
        assert_eq!(
            alloc.stream_kind_for_id(alloc.promotion_vote_stream_id()),
            Some(HadrStreamKind::PromotionVote)
        );

        // Non-allocated stream ID returns None
        assert_eq!(alloc.stream_kind_for_id(999), None);
    }

    #[test]
    fn test_stream_multiplexer_allocate_replicas() {
        let mut mux = StreamMultiplexer::new();

        let alloc0 = mux.allocate_replica_streams(0).unwrap();
        let alloc1 = mux.allocate_replica_streams(1).unwrap();

        assert_eq!(mux.open_stream_count(), 6); // 3 streams per replica

        // Verify multiplexer can look up streams
        assert_eq!(
            mux.lookup_stream(alloc0.heartbeat_stream_id()),
            Some((0, HadrStreamKind::Heartbeat))
        );
        assert_eq!(
            mux.lookup_stream(alloc1.wal_shipping_stream_id()),
            Some((1, HadrStreamKind::WalShipping))
        );
    }

    #[test]
    fn test_stream_multiplexer_duplicate_allocation() {
        let mut mux = StreamMultiplexer::new();

        let _ = mux.allocate_replica_streams(0).unwrap();
        let result = mux.allocate_replica_streams(0); // Should fail

        assert!(result.is_err());
    }

    #[test]
    fn test_stream_multiplexer_close_stream() {
        let mut mux = StreamMultiplexer::new();
        let alloc = mux.allocate_replica_streams(0).unwrap();

        assert!(mux.is_stream_open(alloc.heartbeat_stream_id()));
        assert!(!mux.is_stream_closed(alloc.heartbeat_stream_id()));

        mux.close_stream(alloc.heartbeat_stream_id()).unwrap();

        assert!(!mux.is_stream_open(alloc.heartbeat_stream_id()));
        assert!(mux.is_stream_closed(alloc.heartbeat_stream_id()));
    }

    #[test]
    fn test_orphan_stream_cleanup_idempotent() {
        let mut cleanup = HadrStreamCleanup::new();

        cleanup.mark_orphan(128);
        cleanup.mark_orphan(160);
        cleanup.mark_orphan(192);

        assert_eq!(cleanup.orphan_count(), 3);

        let cleaned1 = cleanup.execute_cleanup();
        assert_eq!(cleaned1.len(), 3);
        assert!(cleaned1.contains(&128));

        // Second cleanup should produce empty set (idempotent)
        let cleaned2 = cleanup.execute_cleanup();
        assert!(cleaned2.is_empty());
    }

    #[test]
    fn test_hadr_stream_kind_bounds() {
        assert_eq!(
            HadrStreamKind::Heartbeat.min_stream_id(),
            HEARTBEAT_STREAM_MIN
        );
        assert_eq!(
            HadrStreamKind::Heartbeat.max_stream_id(),
            HEARTBEAT_STREAM_MAX
        );

        assert_eq!(
            HadrStreamKind::WalShipping.min_stream_id(),
            WAL_SHIPPING_STREAM_MIN
        );
        assert_eq!(
            HadrStreamKind::WalShipping.max_stream_id(),
            WAL_SHIPPING_STREAM_MAX
        );

        assert_eq!(
            HadrStreamKind::PromotionVote.min_stream_id(),
            VOTE_STREAM_MIN
        );
        assert_eq!(
            HadrStreamKind::PromotionVote.max_stream_id(),
            VOTE_STREAM_MAX
        );
    }
}
