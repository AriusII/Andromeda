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
mod result;

pub(crate) use andromeda_observability::{TraceEventFamily, TraceQueryLsnRange, TraceQuerySpec};
pub use durable_audit::{
    DurableAuditTraceQueryResult, DurableAuditTraceQueryRow, DurableAuditTraceQuerySource,
};
pub use result::{TraceQueryMetadata, TraceQueryResult, TraceQueryRow};

#[cfg(test)]
mod tests;
