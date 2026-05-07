//! V0 single-primary WAL shipping contract.
//!
//! Doctrine reminders enforced here:
//! * `visible commit == durable WAL` — replicas only accept records that link
//!   into the durable LSN chain they already hold.
//! * `RAM is never truth` — shipping never mutates WAL byte format; it only
//!   validates pre-existing [`WalRecord`] frames produced by the primary.
//! * V0 topology is single-primary plus replicas. Shipping from a non-primary
//!   role is rejected.
//!
//! This module owns the typed shipping boundary: identities, roles, batch
//! envelopes, ACK tracking, and validation results. Transport dispatch and
//! quorum coordination live outside this module.

mod ack;
mod batch;
mod identity;
mod rejection;

pub use ack::{WalReplicaSafeLsnTracker, WalShipmentAccepted, WalShipmentRange, WalShippingAck};
pub use batch::{WalReplicaExpectation, WalShipmentBatch};
pub use identity::{WalNodeIdentity, WalNodeRole};
pub use rejection::WalShipmentRejection;

#[cfg(test)]
mod tests;
