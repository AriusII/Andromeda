#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Procedure Store

Runtime-free invocation identity, status, and evidence sink contracts.

The crate exposes evidence-only Procedure Store primitives. It does not provide
durable truth, catalog publication, WAL replay, or plan-selection authority.
"#]

mod audit;
mod error;
mod evidence;
mod evidence_role;
mod feedback;
mod feedback_store;
mod history;
mod identity;
mod metrics;
mod registration;
mod regression;
mod runtime_counters;
mod runtime_status;
mod sink;
mod status;

pub use audit::{AuditCorrelation, AuditCorrelationId};
pub use error::{ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult};
pub use evidence::{EvidenceDigest, InvocationEvidenceKind, InvocationEvidenceMarker};
pub use evidence_role::ProcedureStoreEvidenceRole;
pub use feedback::{
    CompletionEvidence, CompletionStatus, FeedbackId, InvocationFeedback, ProcedureFeedback,
    ProcedureFeedbackError,
};
pub use feedback_store::{
    InMemoryProcedureFeedbackStore, ProcedureFeedbackStore, ProcedureFeedbackStoreError,
    RecordOutcome,
};
pub use history::InvocationHistoryRecord;
pub use identity::{InvocationId, InvocationIdentity, ProcedureId};
pub use metrics::{InvocationMetricKind, InvocationMetrics};
pub use registration::ProcedureRegistration;
pub use regression::{
    MAX_REGRESSION_THRESHOLD_BPS, RegressionSeverity, RegressionSignal, RegressionThresholdBps,
};
pub use runtime_counters::ProcedureRuntimeCounters;
pub use runtime_status::ProcedureRuntimeStatus;
pub use sink::InvocationEvidenceSink;
pub use status::InvocationStatus;
