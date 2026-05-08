#![forbid(unsafe_code)]

//! Andromeda columnar analytical descriptor ownership contracts.
//!
//! This crate owns runtime-free descriptors for derived columnar analytical
//! artifacts. It does not persist columnar bytes, publish catalog state, or make
//! columnar output durable truth.
//!
//! Ownership constraints:
//! - Columnar artifacts are derived and advisory unless a later C5 design adds
//!   explicit codecs, WAL coverage, and crash/recovery validation.
//! - Benchmark output, GPU output, RAM state, and temporary files are not truth.
//! - GPU and analytics work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
//! - Adaptive consumers must explain use or rejection through DecisionTrace.

mod descriptor;

pub use descriptor::{
    ColumnarAccelerationPolicy, ColumnarArtifactDescriptor, ColumnarConsumer,
    ColumnarLayoutDescriptor, ColumnarVersionBinding,
};
