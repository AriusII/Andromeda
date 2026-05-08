//! Deadlock detection vocabulary and deterministic wait-for graph.
//!
//! This module is deliberately storage-agnostic and does not wire any abort or
//! rollback policy into the transaction manager. It provides stable
//! transaction-level graph derivation and deterministic cycle evidence.
//!
//! State/event checklist for deterministic detection:
//! - States: empty wait-for graph, graph with waiting edges, no cycle reported,
//!   cycle found, detection timed out, detection deferred for future integration
//!   boundaries only.
//! - Events: derive edges from immutable lock-table snapshot, insert wait edge,
//!   remove wait edge, remove transaction, run detector.
//! - Legal transitions: empty -> edge-bearing on validated edge insertion;
//!   edge-bearing -> empty or reduced graph on removal; detector reports
//!   no-cycle for empty or acyclic graphs, cycle-found for the first cycle
//!   reached by deterministic DFS, or timed-out when an injected deadline has
//!   expired.
//! - Illegal transitions: zero transaction identifiers and self-edges are
//!   rejected; timeout policies must be non-zero and bounded.
//! - Audit: lock-table derivation consumes cloned entries only, all edge storage
//!   is deterministic via ordered maps/sets, DFS starts at sorted transaction ids
//!   and follows sorted blockers, victim selection uses explicit transaction
//!   ordering metadata when requested, clock/deadline reads never sleep, and no
//!   abort decision is emitted by the detector.

#[cfg(test)]
use std::time::Duration;

use andromeda_core::{AndromedaError, AndromedaResult};

mod api;
mod cycle;
mod decision;
mod detector;
mod graph;
mod metadata;
mod policy;
mod validation;
mod victim;

pub use api::*;
pub use decision::*;
pub use detector::*;
pub use graph::*;
pub use metadata::*;
pub use policy::*;
pub use victim::*;

use validation::{
    deadlock_error, validate_cycle_participants, validate_not_self_edge, validate_start_order,
    validate_transaction_id,
};

/// Public deadlock error type.
pub type DeadlockError = AndromedaError;

/// Public deadlock result type.
pub type DeadlockResult<T> = AndromedaResult<T>;

const MISSING_TRANSACTION_ORDERING_METADATA_REASON: &str =
    "deadlock transaction ordering metadata missing for cycle participant";
const DEADLOCK_DETECTION_TIMEOUT_REASON: &str = "deadlock detection deadline expired";

#[cfg(test)]
mod tests;
