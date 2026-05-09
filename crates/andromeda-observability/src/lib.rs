//! Shared observability identifiers and correlation metadata.

#![forbid(unsafe_code)]

mod core_trace;
mod correlation;
mod identity;
mod trace_id;

pub use core_trace::{InvocationTrace, MvccTrace, ResourceTrace};
pub use correlation::{EventCorrelation, ProtocolCorrelation, ProtocolEventScope};
pub use identity::{EventId, EventSchemaVersion, V0_EVENT_SCHEMA_VERSION};
pub use trace_id::TraceId;
