//! MVCC Reclamation Mark Types and Eligibility Criteria
//!
//! This module defines the formal type system for MVCC version reclamation
//! (garbage collection marking). It ensures correctness of version cleanup by
//! checking creator transaction status, visibility timestamps, and grace periods.
//!
//! # Reclamation Lifecycle
//!
//! 1. **Version Creation**: A version is created with:
//!    - `creator_tx_id`: Transaction that created this version
//!    - `version_id`: Unique identifier within the row's version chain
//!    - `begin_ts`: Logical timestamp when this version became visible
//!    - `end_ts`: Initially `u64::MAX` (open/live version)
//!
//! 2. **Version Closure**: When a new version is created or row is deleted:
//!    - Previous version's `end_ts` is set to current logical timestamp
//!    - Marks when this version ceased to be the "current" version
//!
//! 3. **Eligibility Check**: GC scanner periodically checks if a version is eligible:
//!    - Is the creator transaction committed? (from `TransactionStatusTable`)
//!    - Is `end_ts` invisible to all active snapshots? (`end_ts < min_visible_ts`)
//!    - Has enough time passed since marking? (grace period)
//!
//! 4. **Reclamation Marking**: If all eligibility criteria are met:
//!    - Emit `ReclamationMark` with metadata
//!    - Generate `ReclamationCommand` for executor
//!
//! 5. **Reclamation Processing**: Executor processes command:
//!    - Remove version tuple from storage
//!    - Decrement row's version count
//!    - Update statistics
//!
//! # Invariant: No Visible Version Can Be Reclaimed
//!
//! The eligibility check ensures this critical invariant:
//! - A version is visible if: creator is committed AND end_ts >= snapshot.timestamp
//! - Therefore, a version can only be reclaimed if: end_ts < min_visible_ts
//! - Since `min_visible_ts` is the oldest active snapshot's timestamp,
//!   no active snapshot can see `end_ts` (they all see timestamps >= min_visible_ts)
//! - Uncommitted creators also cannot be reclaimed (reads must wait for durable commit)

mod command;
mod eligibility;
mod mark;
mod stats;

#[cfg(test)]
mod tests;

/// Unique identifier for a version within a row's version chain.
pub type VersionId = u64;

/// Logical timestamp for MVCC ordering (not wall-clock time).
pub type Timestamp = u64;

pub use command::ReclamationCommand;
pub use eligibility::ReclamationEligibility;
pub use mark::{ReclamationMark, ReclamationMarkCandidate};
pub use stats::ReclamationStats;
