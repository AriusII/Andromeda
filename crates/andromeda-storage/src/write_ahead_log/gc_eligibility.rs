//! Compatibility reexports for WAL GC eligibility checks.
//!
//! Canonical eligibility logic now lives in
//! `andromeda_wal::write_ahead_log::gc_eligibility`.

pub use andromeda_wal::write_ahead_log::gc_eligibility::{EligibilityResult, GcEligibilityChecker};

#[cfg(test)]
mod tests;
