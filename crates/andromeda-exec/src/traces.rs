//! Compatibility facade for execution trace ownership.
//!
//! `andromeda-execution-trace` owns invocation trace events and audit-ledger
//! contracts. `andromeda-exec` reexports those types so execution callers keep a
//! stable surface while trace ownership lives in its dedicated crate.

pub use andromeda_execution_trace::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
