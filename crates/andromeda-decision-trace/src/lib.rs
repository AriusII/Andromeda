#![forbid(unsafe_code)]

//! Future owner crate scaffold for runtime-free Andromeda DecisionTrace
//! contracts.
//!
//! This crate intentionally defines no public API yet. Current trace behavior
//! remains in `andromeda-observe` until a later extraction registers this
//! package in the workspace and proves compatibility.
//!
//! Ownership constraints:
//! - DecisionTrace explains decisions after the fact. It is not storage,
//!   catalog, transaction, recovery, or security truth.
//! - Adaptive traces must carry version bindings and stale-evidence status.
//! - Benchmark output, ScenarioEvidence, analytics output, and GPU output remain
//!   advisory even when represented in a trace.
//! - Trace contracts must stay runtime-free and redaction-safe.
