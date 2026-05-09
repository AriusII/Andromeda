//! Storage operational profiles.
//!
//! Operational profiles bind hardware descriptors to storage workflow budgets
//! for admission checks. They are storage-local guardrail presets, not hardware
//! ownership and not durable truth. Hardware primitives remain owned by
//! `andromeda-hardware`; durable state remains owned by
//! WAL, manifest, page, and segment crates.

mod budgets;
mod constants;
mod errors;
mod hardware;
mod profile;
mod workflow;

pub use profile::OperationalProfile;
pub use workflow::{IoWorkflowProfile, OperationalProfileMode};

#[cfg(test)]
mod tests;
