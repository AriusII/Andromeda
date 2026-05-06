//! Administration-only trace query contract.
//!
//! Accepts typed filters over recorded [`EventEnvelope`] metadata and payload
//! evidence. Formatting remains a caller concern.
//!
//! [`EventEnvelope`]: crate::EventEnvelope

mod durable_audit;
mod event_family;
mod filtering;
mod in_memory;
mod permission;
mod result;
mod spec;

pub use durable_audit::{
    DurableAuditTraceQueryResult, DurableAuditTraceQueryRow, DurableAuditTraceQuerySource,
};
pub use event_family::TraceEventFamily;
pub use permission::TraceQueryPermissionMatrix;
pub use result::{TraceQueryMetadata, TraceQueryResult, TraceQueryRow};
pub use spec::{
    TRACE_QUERY_DEFAULT_LIMIT, TRACE_QUERY_MAX_LIMIT, TraceQueryFilter, TraceQueryLsnRange,
    TraceQuerySpec,
};

#[cfg(test)]
mod tests;
