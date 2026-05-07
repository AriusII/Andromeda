//! WAL segment reclaimability facade.
//!
//! This module keeps the public reclaimability contract stable while splitting
//! the implementation into focused parts:
//! - `decision`: observable decision and diagnostic helpers
//! - `evidence`: immutable inputs used for one deterministic decision
//! - `policy`: policy facade and default implementation
//! - `validation`: boundary consistency checks
//!
//! Safety invariants:
//! - a segment required for crash recovery is never reclaimed
//! - active snapshot, replica, and PITR boundaries are evaluated from one
//!   captured policy value
//! - each decision exposes a specific blocking reason for audit traces
//! - no unsafe code

mod boundary;
mod decision;
mod evidence;
mod policy;

pub use boundary::RetentionBoundaryPolicy;
pub use decision::ReclaimabilityDecision;
pub use evidence::ReclaimabilityEvidence;
pub use policy::{DefaultReclaimabilityPolicy, WalSegmentReclaimability};

#[cfg(test)]
mod tests;
