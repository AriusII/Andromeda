#![forbid(unsafe_code)]

//! Cross-commit benchmark history record model and query types.
//!
//! Records are immutable once created and are serialized to JSON Lines for
//! portable artifact storage. This module never modifies benchmark state; it
//! only models and queries history.
//!
//! ## Invariants
//!
//! - History records are append-only; no mutation after creation.
//! - Queries over the same record set with the same parameters are deterministic.
//! - All records are serializable to JSON Lines for artifact portability.

mod advisory;
mod json;
mod query;
mod record;

pub use advisory::BenchmarkHistoryAdvisoryMetadata;
pub use query::{HistoryQuery, HistoryQueryResult, TimeRange};
pub use record::BenchmarkHistoryRecord;

#[cfg(test)]
mod tests;
