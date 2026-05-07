//! Crash recovery implementation scenario suite.
//!
//! The suite keeps the original 40 scenario contracts but splits them by
//! recovery behavior:
//!
//! - crash-before-commit decisions for incomplete and rolled-back transactions
//! - committed transactional replay
//! - non-transactional redo replay
//! - redo-floor and recovered transaction-id boundaries
//!
//! Shared fixtures build typed manifests, WAL records, and decision
//! expectations so each scenario stays focused on its crash point, durable
//! state, expected replay decision, and observable recovery output.

#[path = "crash_recovery_impl/boundary_conditions.rs"]
mod boundary_conditions;
#[path = "crash_recovery_impl/committed_transactions.rs"]
mod committed_transactions;
#[path = "crash_recovery_impl/crash_before_commit.rs"]
mod crash_before_commit;
#[path = "crash_recovery_impl/fixtures.rs"]
mod fixtures;
#[path = "crash_recovery_impl/non_transactional_records.rs"]
mod non_transactional_records;
