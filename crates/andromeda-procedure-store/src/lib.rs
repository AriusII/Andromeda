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
mod feedback;
mod history;
mod identity;
mod metrics;
mod regression;
mod sink;
mod status;

pub use audit::{AuditCorrelation, AuditCorrelationId};
pub use error::{ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult};
pub use evidence::{EvidenceDigest, InvocationEvidenceKind, InvocationEvidenceMarker};
pub use feedback::{FeedbackId, InvocationFeedback};
pub use history::InvocationHistoryRecord;
pub use identity::{InvocationId, InvocationIdentity, ProcedureId};
pub use metrics::{InvocationMetricKind, InvocationMetrics};
pub use regression::{
    MAX_REGRESSION_THRESHOLD_BPS, RegressionSeverity, RegressionSignal, RegressionThresholdBps,
};
pub use sink::InvocationEvidenceSink;
pub use status::InvocationStatus;
