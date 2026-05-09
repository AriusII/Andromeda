use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::kind::HadrStreamKind;
use super::{
    HADR_STREAM_STRIDE, HADR_STREAMS_PER_REPLICA, HEARTBEAT_MAX_REPLICAS, HEARTBEAT_STREAM_MIN,
    HadrLogicalStreamId, VOTE_MAX_REPLICAS, VOTE_STREAM_MIN, WAL_SHIPPING_MAX_REPLICAS,
    WAL_SHIPPING_STREAM_MIN,
};

/// Stream allocation information.
///
/// This struct holds deterministically allocated stream IDs for a replica
/// and provides lookup methods for stream kind resolution.
#[derive(Debug, Clone)]
pub struct StreamAllocation {
    /// Replica index in the topology (0-based).
    replica_index: u64,

    /// Heartbeat logical stream ID.
    heartbeat_stream_id: HadrLogicalStreamId,

    /// WAL shipping logical stream ID.
    wal_shipping_stream_id: HadrLogicalStreamId,

    /// Promotion vote logical stream ID.
    promotion_vote_stream_id: HadrLogicalStreamId,
}

impl StreamAllocation {
    /// Creates a new stream allocation for a replica at the given index.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the replica index exceeds the maximum supported replicas.
    pub fn new(replica_index: u64) -> AndromedaResult<Self> {
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

        let heartbeat_stream_id =
            HadrLogicalStreamId::new(HEARTBEAT_STREAM_MIN + replica_index * HADR_STREAM_STRIDE);
        let wal_shipping_stream_id =
            HadrLogicalStreamId::new(WAL_SHIPPING_STREAM_MIN + replica_index * HADR_STREAM_STRIDE);
        let promotion_vote_stream_id =
            HadrLogicalStreamId::new(VOTE_STREAM_MIN + replica_index * HADR_STREAM_STRIDE);

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

    /// Returns the heartbeat logical stream ID for this replica.
    pub const fn heartbeat_logical_stream_id(&self) -> HadrLogicalStreamId {
        self.heartbeat_stream_id
    }

    /// Returns the WAL shipping logical stream ID for this replica.
    pub const fn wal_shipping_logical_stream_id(&self) -> HadrLogicalStreamId {
        self.wal_shipping_stream_id
    }

    /// Returns the promotion vote logical stream ID for this replica.
    pub const fn promotion_vote_logical_stream_id(&self) -> HadrLogicalStreamId {
        self.promotion_vote_stream_id
    }

    /// Returns the heartbeat stream ID value for this replica.
    pub const fn heartbeat_stream_id(&self) -> u64 {
        self.heartbeat_stream_id.get()
    }

    /// Returns the WAL shipping stream ID value for this replica.
    pub const fn wal_shipping_stream_id(&self) -> u64 {
        self.wal_shipping_stream_id.get()
    }

    /// Returns the promotion vote stream ID value for this replica.
    pub const fn promotion_vote_stream_id(&self) -> u64 {
        self.promotion_vote_stream_id.get()
    }

    pub(super) fn stream_ids_with_kinds(
        &self,
    ) -> [(HadrLogicalStreamId, HadrStreamKind); HADR_STREAMS_PER_REPLICA] {
        [
            (self.heartbeat_stream_id, HadrStreamKind::Heartbeat),
            (self.wal_shipping_stream_id, HadrStreamKind::WalShipping),
            (self.promotion_vote_stream_id, HadrStreamKind::PromotionVote),
        ]
    }

    /// Looks up the stream kind for a given stream ID.
    ///
    /// Returns the kind if the stream ID is allocated to this replica;
    /// returns `None` if the stream ID does not belong to this allocation.
    pub fn stream_kind_for_id(&self, stream_id: u64) -> Option<HadrStreamKind> {
        let logical_stream_id = HadrLogicalStreamId::new(stream_id);
        self.stream_ids_with_kinds()
            .into_iter()
            .find_map(|(id, kind)| (id == logical_stream_id).then_some(kind))
    }
}
