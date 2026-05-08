#![forbid(unsafe_code)]

mod cluster_security;
mod fencing;
mod membership_persistence_format;
mod membership_state_validation;
mod membership_store;
pub mod membership_transitions;
mod promotion_boundary;
mod promotion_decision_helpers;
mod promotion_quorum_evidence;
mod quorum;
pub mod quorum_runtime;
pub mod shipping_runtime;
mod types;

pub use cluster_security::*;
pub use fencing::*;
pub use membership_store::*;
pub use membership_transitions::*;
pub use promotion_boundary::*;
pub use quorum::*;
pub use quorum_runtime::{
    FencingDecision, FencingEvent, FencingPolicy, PromotionRank, QuorumConsensus, QuorumMembership,
    ReplicaHealthState, ReplicaMember, ReplicationMode, decide_fencing, rank_promotion_candidates,
    select_promotion_candidate,
};
pub use shipping_runtime::{
    LsnCorrelationState, ShippingBackpressureRequest, ShippingCondition, ShippingSegmentDescriptor,
};
pub use types::*;

pub(crate) use andromeda_wal::Lsn;

pub(crate) mod write_ahead_log {
    pub(crate) use andromeda_wal::write_ahead_log::record::WalRecord;

    #[cfg(test)]
    pub(crate) mod record {
        pub(crate) use andromeda_wal::write_ahead_log::record::{WalRecord, WalRecordKind};
    }
}
