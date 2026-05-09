use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::{HashMap, HashSet};

use super::HADR_STREAMS_PER_REPLICA;
use super::HadrLogicalStreamId;
use super::allocation::StreamAllocation;
use super::kind::HadrStreamKind;

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
    /// Map of logical stream ID to allocated stream information.
    stream_allocations: HashMap<HadrLogicalStreamId, (u64, HadrStreamKind)>,

    /// Set of open (non-terminal) logical stream IDs.
    open_streams: HashSet<HadrLogicalStreamId>,

    /// Set of closed (terminal) logical stream IDs.
    closed_streams: HashSet<HadrLogicalStreamId>,

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
            max_concurrent_streams: max_concurrent,
            ..Self::new()
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
        let allocation_entries = allocation.stream_ids_with_kinds();

        if allocation_entries
            .iter()
            .any(|(stream_id, _)| self.stream_allocations.contains_key(stream_id))
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!("replica {} already has allocated streams", replica_index),
            ));
        }

        if self.open_streams.len() + HADR_STREAMS_PER_REPLICA > self.max_concurrent_streams {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "allocating new replica streams would exceed concurrency limit",
            ));
        }

        for (stream_id, kind) in allocation_entries {
            self.stream_allocations
                .insert(stream_id, (replica_index, kind));
            self.open_streams.insert(stream_id);
        }

        Ok(allocation)
    }

    /// Looks up the stream kind and replica index for a given stream ID.
    ///
    /// Returns `(replica_index, stream_kind)` if the stream is allocated;
    /// returns `None` otherwise.
    pub fn lookup_stream(&self, stream_id: u64) -> Option<(u64, HadrStreamKind)> {
        self.stream_allocations
            .get(&HadrLogicalStreamId::new(stream_id))
            .copied()
    }

    /// Marks a stream as closed (terminal).
    ///
    /// Once closed, the stream ID can be reused after terminal state is drained.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the stream ID is not allocated.
    pub fn close_stream(&mut self, stream_id: u64) -> AndromedaResult<()> {
        let logical_stream_id = HadrLogicalStreamId::new(stream_id);
        if !self.stream_allocations.contains_key(&logical_stream_id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!("stream ID {} is not allocated", stream_id),
            ));
        }

        self.open_streams.remove(&logical_stream_id);
        self.closed_streams.insert(logical_stream_id);
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
        self.open_streams
            .contains(&HadrLogicalStreamId::new(stream_id))
    }

    /// Returns true if a stream ID is closed.
    pub fn is_stream_closed(&self, stream_id: u64) -> bool {
        self.closed_streams
            .contains(&HadrLogicalStreamId::new(stream_id))
    }
}

impl Default for StreamMultiplexer {
    fn default() -> Self {
        Self::new()
    }
}
