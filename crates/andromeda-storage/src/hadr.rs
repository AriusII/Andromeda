//! Compatibility facade for HADR ownership.
//!
//! HADR membership, quorum, shipping, and promotion decision logic now lives in
//! `andromeda-hadr`. Storage keeps this module as an explicit API boundary so
//! consumers can continue to import `andromeda_storage::hadr` while moving toward
//! a narrower public surface.

// Explicit public façade for storage-exposed HADR surface.
pub use andromeda_hadr::{
    FailoverTrigger, FileBackedHadrMembershipStore, HadrClusterManifestUpdateEvidence,
    HadrClusterManifestUpdateRequest, HadrClusterManifestVersion, HadrClusterOperation,
    HadrClusterSecurityEvidence, HadrEpoch, HadrFencingContext, HadrFencingToken,
    HadrMembershipRecord, HadrMembershipSnapshot, HadrMembershipStore, HadrNodeId, HadrNodeRole,
    HadrNodeState, HadrPromotionAuditLog, HadrPromotionAuditMarker, HadrPromotionAuditReceipt,
    HadrPromotionOutcome, HadrPromotionRejection, HadrPromotionRequest, HadrPromotionVote,
    HadrQuorumMembership, NoopPromotionAuditLog, PromotionAttempt, PromotionBoundary,
    PromotionCandidate, PromotionCommit, PromotionEligibility, PromotionPlanner,
    PromotionRequirements, enforce_fencing_token, evaluate_promotion, is_promotion_eligible,
    select_best_eligible_candidate,
};

// Compatibility aliases for nested module imports used by existing clients.
pub mod membership_transitions {
    pub use andromeda_hadr::membership_transitions::{
        MembershipState, MembershipStateTracker, StateTransitionRecord, TransitionError,
        TransitionEvent, transition,
    };
}

pub mod quorum_runtime {
    pub use andromeda_hadr::quorum_runtime::{
        FencingDecision, FencingEvent, FencingPolicy, PromotionRank, QuorumConsensus,
        QuorumMembership, QuorumMembershipRejection, ReplicaHealthState, ReplicaMember,
        ReplicationMode, decide_fencing, rank_promotion_candidates, select_promotion_candidate,
    };
}

pub mod shipping_runtime {
    pub use andromeda_hadr::shipping_runtime::{
        LsnCorrelationState, ShippingBackpressureRequest, ShippingCondition,
        ShippingSegmentDescriptor,
    };
}
