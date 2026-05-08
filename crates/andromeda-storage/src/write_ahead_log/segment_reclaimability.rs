//! Compatibility reexports for WAL segment reclaimability ownership.
//!
//! Canonical reclaimability policy/types now live in
//! `andromeda_wal::write_ahead_log::segment_reclaimability`.

pub use andromeda_wal::write_ahead_log::segment_reclaimability::{
    DefaultReclaimabilityPolicy, ReclaimabilityDecision, ReclaimabilityEvidence,
    RetentionBoundaryPolicy, WalReplicaSafeLsnBoundaryProvider, WalSegmentReclaimability,
};

#[cfg(test)]
mod tests;
