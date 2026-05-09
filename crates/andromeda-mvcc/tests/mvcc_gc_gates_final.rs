//! MVCC GC production gate tests.
//!
//! Comprehensive validation of MVCC garbage collection before runtime integration.
//!
//! Tests validate:
//! - Eligibility criteria: creator committed, end_ts invisible, closed versions
//! - Scheduler behavior: triggering, waking on threshold, reclamation work processing
//! - Integration: table scans, heap space reclamation, long-running transaction blocking
//! - Concurrency: concurrent writers + GC scheduler, interference-free reads

#[path = "mvcc_gc_gates_final/concurrency.rs"]
mod concurrency;
#[path = "mvcc_gc_gates_final/edge_cases.rs"]
mod edge_cases;
#[path = "mvcc_gc_gates_final/eligibility.rs"]
mod eligibility;
#[path = "mvcc_gc_gates_final/fixtures.rs"]
mod fixtures;
#[path = "mvcc_gc_gates_final/integration.rs"]
mod integration;
#[path = "mvcc_gc_gates_final/metrics.rs"]
mod metrics;
#[path = "mvcc_gc_gates_final/scheduler_stats.rs"]
mod scheduler_stats;
