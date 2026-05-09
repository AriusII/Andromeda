//! Compatibility reexports for the WAL shipping contract.
//!
//! Canonical shipping contract types now live in
//! `andromeda_hadr::shipping_contract`.

pub use andromeda_hadr::shipping_contract::{
    WalNodeIdentity, WalNodeRole, WalReplicaExpectation, WalReplicaSafeLsnTracker,
    WalShipmentAccepted, WalShipmentBatch, WalShipmentRange, WalShipmentRejection, WalShippingAck,
};

#[cfg(test)]
mod tests;
