//! Shared observability identifiers and correlation metadata.

#![forbid(unsafe_code)]

macro_rules! define_observability_u128_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u128);

        impl $name {
            pub const fn new(value: u128) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u128 {
                self.0
            }

            pub const fn is_zero(self) -> bool {
                self.0 == 0
            }
        }
    };
}

pub(crate) use define_observability_u128_id;

mod core_trace;
mod correlation;
mod critical_decision;
mod exporter;
mod identity;
mod protocol_rejection;
mod protocol_trace;
mod query;
mod trace_id;
mod transition_trace;

pub use core_trace::{InvocationTrace, MvccTrace, ResourceTrace};
pub use correlation::{EventCorrelation, ProtocolCorrelation, ProtocolEventScope};
pub use critical_decision::{CriticalDecisionKind, CriticalDecisionTrace};
pub use exporter::{
    ExportDecisionTrace, ExporterBackend, ExporterConfig, ExporterTrait, Metric, MockExporter,
    RetryPolicy,
};
pub use identity::{EventId, EventSchemaVersion, V0_EVENT_SCHEMA_VERSION};
pub use protocol_rejection::{
    ProtocolRejectionReason, ProtocolRejectionTrace, ProtocolSurfacePlane,
};
pub use protocol_trace::{
    AuthorizationDeniedTrace, BackpressureTrace, CompletionEmittedTrace, ContractRejectedTrace,
    FrameRejectionTrace, SchemaLayoutDecisionTrace, StreamRoleRejectionTrace,
    UnsupportedVersionTrace,
};
pub use query::{
    TRACE_QUERY_DEFAULT_LIMIT, TRACE_QUERY_MAX_LIMIT, TraceEventFamily, TraceEventFamilyClassified,
    TraceQueryFilter, TraceQueryLsnRange, TraceQuerySpec, TraceQueryValidation,
    TraceQueryValidationMessages, trace_query_filter_is_unbounded, validate_trace_query_parts,
};
pub use trace_id::TraceId;
pub use transition_trace::{
    ExecutionTransitionTrace, TransactionPhaseCode, TransactionTransitionTrace,
    TransitionReasonCode,
};

pub type DecisionTrace = CriticalDecisionTrace;
