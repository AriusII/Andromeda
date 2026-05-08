#![forbid(unsafe_code)]

//! Advisory Andromeda batch analytics ownership contracts.
//!
//! This crate owns runtime-free descriptors for analytical jobs that can feed
//! statistics, Maps, benchmark review, and operational diagnostics. It does not
//! execute those jobs or make their output durable truth.
//!
//! Ownership constraints:
//! - Analytical output is advisory until a runtime owner validates and publishes
//!   a version-bound result.
//! - Analytics must not enter C5 commit, WAL, rollback, recovery, MVCC
//!   short-visibility, catalog publication, or security-critical paths.
//! - GPU acceleration must be optional, disableable, and backed by CPU fallback.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.

mod boundary;

pub use boundary::{
    AdvisoryAnalyticsJob, AnalyticsAccelerationPolicy, AnalyticsExecutionBounds,
    AnalyticsJobDescriptor, AnalyticsWorkloadKind,
};
