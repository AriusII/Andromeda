#![forbid(unsafe_code)]

pub mod dispatch;
mod executor_bridge;
mod helpers;
mod invocation;
mod local;
pub mod services;
mod srpl_dispatch;
mod surface_gate;

pub use andromeda_admission::{
    ExecutionIoAdmissionDecision, ExecutionIoAdmissionRequest, InvocationContext,
};
pub use andromeda_execution::{
    FieldValue, LocalProcedure, ProcedureHandler, ProcedureRegistry, SrplExecutionAdapter,
    SrplStreamBackpressure, SrplTransactionContext, SrplTypedEnvironment, StructuredObject,
};
pub use andromeda_execution_trace::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
pub use andromeda_procedure_runtime::{DefaultResultMetadataExtractor, ResultMetadataExtractor};
pub use andromeda_result_stream::{
    BackpressuredResultStream, COMPLETION_ENVELOPE_VERSION, CompletionStatus,
    DEFAULT_RESULT_STREAM_CAPACITY, InvocationCompletion, MAX_RESULT_STREAM_CAPACITY,
    MIN_RESULT_STREAM_CAPACITY, ResultStreamMetadata, ResultStreamMetrics, StreamCompletion,
};
pub use andromeda_retry::{ErrorRetryability, RetryAttempt, RetryDecision, RetryPolicy};
pub use dispatch::{
    LocalDispatchPlan, LocalDispatchReceipt, LocalDispatcher, LocalRollbackPlan,
    LocalRollbackReceipt, PermissionScopeValidation, PreTransactionDispatchEvidence,
    ProcedureDispatchRequest, ProcedureDispatchUnavailableReason, ProcedureDispatcher,
    RemoteProcedureDispatcherUnavailable, RollbackCause, RollbackWalDurabilityEvidence,
    WalDurabilityEvidence, catalog_backed_srpl_dispatcher_adapter, validate_dispatch_permissions,
    validate_dispatch_permissions_or_error,
};
pub use executor_bridge::ExecutorDispatchBridge;
#[allow(deprecated)]
pub use helpers::transaction_id_for_invocation;
pub use invocation::{InvocationReject, InvocationRequest, ProcedureInvoker};
pub use local::{
    LocalVerticalRuntime, VerticalInvocationOutcome, require_local_procedure_execution_io_admission,
};
pub use services::{
    AdmissionService, CompletionEmission, CompletionMappingService, ErrorKind,
    InvocationCompletionEmitter, PreTransactionValidationService, ResultValidationService,
    RetryRouting, RoutedTransactionError, TerminalTxEvidence, TerminalTxJournal, TerminalTxState,
    route_transaction_error,
};
pub use srpl_dispatch::SrplProcedureDispatcher;
pub use surface_gate::{
    AuthorizedProcedureDispatch, SurfacePlaneAuthorizer, surface_plane_to_scope,
};
