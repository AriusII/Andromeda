//! Shared observability identifiers and correlation metadata.

#![forbid(unsafe_code)]

mod correlation;
mod identity;
mod trace_id;

pub use correlation::{EventCorrelation, ProtocolCorrelation, ProtocolEventScope};
pub use identity::{EventId, EventSchemaVersion, V0_EVENT_SCHEMA_VERSION};
pub use trace_id::TraceId;
