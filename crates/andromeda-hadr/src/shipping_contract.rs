//! V0 single-primary WAL shipping contract.
//!
//! This module owns the transport-independent WAL shipping boundary:
//! identities, roles, batch envelopes, ACK tracking, and validation results.
//! Runtime dispatch and quorum coordination live in adjacent HADR modules.

mod batch;
mod evidence;
mod identity;
mod rejection;
mod resync;
mod tracker;

pub use batch::{WalReplicaExpectation, WalShipmentAccepted, WalShipmentBatch, WalShipmentRange};
pub use evidence::{
    WalShippingEvidenceCodecRejection, WalShippingEvidenceRejectionReason, WalShippingEvidenceV0,
};
pub use identity::{WalNodeIdentity, WalNodeRole};
pub use rejection::WalShipmentRejection;
pub use resync::{
    ReplicaPromotionBlocker, ReplicaPromotionEligibility, ReplicaResyncDecision,
    ReplicaResyncEvidence, ResyncCompatibility, ResyncLagStatus,
};
pub use tracker::{WalReplicaSafeLsnTracker, WalShippingAck, WalShippingAckBindingRejection};

#[cfg(test)]
mod tests;
