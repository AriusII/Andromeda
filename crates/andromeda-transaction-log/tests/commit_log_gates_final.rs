//! Commit log production gate tests.
//!
//! Comprehensive validation of commit log durability and visibility guarantees
//! before runtime integration.
//!
//! Tests validate:
//! - Five-step commit sequence: WAL write, flush, status update, visibility
//! - Durability invariants: crash-before/after-flush scenarios
//! - Ordering guarantees: strictly increasing LSNs, no time warps
//! - Concurrency: concurrent commits with correct ordering and visibility

#[path = "commit_log_gates_final/concurrency.rs"]
mod concurrency;
#[path = "commit_log_gates_final/durability_ordering.rs"]
mod durability_ordering;
#[path = "commit_log_gates_final/edge_cases.rs"]
mod edge_cases;
#[path = "commit_log_gates_final/five_step.rs"]
mod five_step;
#[path = "commit_log_gates_final/fixtures.rs"]
mod fixtures;
#[path = "commit_log_gates_final/metadata_gc.rs"]
mod metadata_gc;
#[path = "commit_log_gates_final/visibility.rs"]
mod visibility;

mod support;
