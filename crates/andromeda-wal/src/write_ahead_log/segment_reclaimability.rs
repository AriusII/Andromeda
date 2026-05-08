//! WAL segment reclaimability policy facade.
//!
//! This module owns deterministic reclaimability decisions and retention
//! boundary policy.

mod boundary;
mod decision;
mod evidence;
mod policy;

pub use boundary::RetentionBoundaryPolicy;
pub use decision::ReclaimabilityDecision;
pub use evidence::ReclaimabilityEvidence;
pub use policy::{
    DefaultReclaimabilityPolicy, WalReplicaSafeLsnBoundaryProvider, WalSegmentReclaimability,
};

#[cfg(test)]
mod tests;
