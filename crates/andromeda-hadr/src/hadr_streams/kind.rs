use super::{
    HEARTBEAT_MAX_REPLICAS, HEARTBEAT_STREAM_MAX, HEARTBEAT_STREAM_MIN, RESERVED_STREAM_MAX,
    RESERVED_STREAM_MIN, VOTE_MAX_REPLICAS, VOTE_STREAM_MAX, VOTE_STREAM_MIN,
    WAL_SHIPPING_MAX_REPLICAS, WAL_SHIPPING_STREAM_MAX, WAL_SHIPPING_STREAM_MIN,
};

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
            Self::Control => RESERVED_STREAM_MAX - RESERVED_STREAM_MIN + 1,
        }
    }
}
