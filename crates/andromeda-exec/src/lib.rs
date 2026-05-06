#![forbid(unsafe_code)]

mod admission;
mod business;
pub mod dispatch;
mod executor_bridge;
mod helpers;
mod invocation;
mod local;
mod registry;
mod result;
mod result_metadata_extractor;
mod result_stream;
pub mod retry;
pub mod services;
mod srpl_adapters;
mod srpl_dispatch;
mod surface_gate;
pub mod traces;
mod vertical_slice_entry;
mod wal_evidence;

pub use admission::*;
pub use business::*;
pub use dispatch::{
    LocalDispatchPlan, LocalDispatchReceipt, LocalDispatcher, LocalRollbackPlan,
    LocalRollbackReceipt, PermissionScopeValidation, PreTransactionDispatchEvidence,
    ProcedureDispatchRequest, ProcedureDispatchUnavailableReason, ProcedureDispatcher,
    RemoteProcedureDispatcherUnavailable, RollbackCause, RollbackWalDurabilityEvidence,
    WalDurabilityEvidence, validate_dispatch_permissions, validate_dispatch_permissions_or_error,
};
pub use executor_bridge::ExecutorDispatchBridge;
#[allow(deprecated)]
pub use helpers::transaction_id_for_invocation;
pub use invocation::*;
pub use local::*;
pub use registry::{
    InventoryQueryStockProcedureHandler, InventoryReleaseStockProcedureHandler, ProcedureHandler,
    ProcedureRegistry, ReserveStockProcedureHandler,
};
pub use result::*;
pub use result_metadata_extractor::{DefaultResultMetadataExtractor, ResultMetadataExtractor};
pub use result_stream::{
    BackpressuredResultStream, DEFAULT_RESULT_STREAM_CAPACITY, MAX_RESULT_STREAM_CAPACITY,
    MIN_RESULT_STREAM_CAPACITY, ResultStreamMetrics, StreamCompletion,
};
pub use retry::{ErrorRetryability, RetryAttempt, RetryDecision, RetryPolicy};
pub use services::{
    AdmissionService, CompletionEmission, CompletionMappingService, ErrorKind,
    InvocationCompletionEmitter, PreTransactionValidationService, ResultValidationService,
    RetryRouting, RoutedTransactionError, TerminalTxEvidence, TerminalTxJournal, TerminalTxState,
    route_transaction_error,
};
pub use srpl_adapters::{
    FieldValue, SrplExecutionAdapter, SrplStreamBackpressure, SrplTransactionContext,
    SrplTypedEnvironment, StructuredObject,
};
pub use srpl_dispatch::SrplProcedureDispatcher;
pub use surface_gate::{
    AuthorizedProcedureDispatch, SurfacePlaneAuthorizer, surface_plane_to_scope,
};
pub use traces::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
pub use vertical_slice_entry::*;
pub use wal_evidence::*;
