#![forbid(unsafe_code)]

mod admission;
pub mod business;
pub mod compat;
pub mod dispatch;
mod executor_bridge;
mod helpers;
mod invocation;
mod local;
mod registry;
pub mod result;
mod result_metadata_extractor;
mod result_stream;
/// Compatibility facade for retry types.
///
/// Kept as a public module so callers can continue using
/// `andromeda_exec::retry` after ownership moved to `andromeda-retry`.
pub mod retry;
pub mod services;
mod srpl_adapters;
mod srpl_dispatch;
mod surface_gate;
/// Compatibility facade for execution trace types.
///
/// Kept as a public module so callers can continue using
/// `andromeda_exec::traces` after ownership moved to
/// `andromeda-execution-trace`.
pub mod traces;
mod vertical_slice_entry;
mod wal_evidence;

pub use admission::{ExecutionIoAdmissionDecision, ExecutionIoAdmissionRequest, InvocationContext};
pub use business::{
    HeapInventoryProductStockStore, INVENTORY_QUERY_STOCK_COLUMN_COUNT,
    INVENTORY_QUERY_STOCK_RESULT_STREAM_ID, INVENTORY_RELEASE_STOCK_EXACT_RESULT_ROWS,
    INVENTORY_RELEASE_STOCK_RESERVATION_ROWS_FREED, INVENTORY_RELEASE_STOCK_RESULT_STREAM_ID,
    INVENTORY_RELEASE_STOCK_STOCK_ROWS_AFFECTED, INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS,
    INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED, INVENTORY_RESERVE_STOCK_RESULT_STREAM_ID,
    INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED, InventoryBusinessMvccStore,
    InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence,
    InventoryProductStockReservationIntent, InventoryProductStockStore, InventoryReservation,
    InventoryReserveStockExecutor, InventoryReserveStockMvccDecision,
    InventoryReserveStockMvccEvidence, InventoryReserveStockRejectionEvidence,
    InventoryReserveStockResultEvidence, InventoryStock, InventoryStockVersionEvidence,
    ObservedInventoryProductStockStore, QueryStockCommand, QueryStockEffect, ReleaseStockCommand,
    ReleaseStockEffect, ReservationResult, ReserveStockCommand, ReserveStockEffect,
};
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
pub use invocation::{InvocationReject, InvocationRequest, ProcedureInvoker};
pub use local::{
    LocalHeapRowInsertRedoTemplate, LocalHeapRowRedoContractBinding, LocalProcedure,
    LocalVerticalRuntime, VerticalInvocationOutcome,
    require_local_procedure_execution_io_admission,
};
pub use registry::{
    InventoryQueryStockProcedureHandler, InventoryReleaseStockProcedureHandler, ProcedureHandler,
    ProcedureRegistry, ReserveStockProcedureHandler,
};
pub use result::{
    COMPLETION_ENVELOPE_VERSION, CompletionStatus, InvocationCompletion, ResultStreamMetadata,
};
pub use result_metadata_extractor::{DefaultResultMetadataExtractor, ResultMetadataExtractor};
pub use result_stream::{
    BackpressuredResultStream, DEFAULT_RESULT_STREAM_CAPACITY, MAX_RESULT_STREAM_CAPACITY,
    MIN_RESULT_STREAM_CAPACITY, ResultStreamMetrics, StreamCompletion,
};
/// Re-export of stable retry types from `andromeda-retry`.
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
/// Re-export of stable trace types from `andromeda-execution-trace`.
pub use traces::{AuditLedger, InMemoryAuditLedger, InvocationTraceEvent};
pub use vertical_slice_entry::{
    V0InventoryProtocolViolation, V0InventoryRecoverableOutcome, V0InventoryRecoverableRuntime,
    V0InventoryReserveStockExecutableProcedure, V0InventoryReserveStockRpcPayload,
    bind_inventory_reserve_stock_v0_pdf_executable_procedure, decode_v0_execute_frame,
    emit_v0_inventory_reserve_stock_pre_transaction_refusal,
    encode_inventory_reserve_stock_v0_execute_frame,
};
pub use wal_evidence::{
    CommitLogInvocationWal, DurableExecWalPrefix, EXEC_TX_COMMIT_PAYLOAD_LEN,
    EXEC_TX_ROLLBACK_PAYLOAD_LEN, InvocationWal, TxReplayBridgeEvidence,
    TxReplayFromExecWalEvidence, encode_exec_tx_commit_payload, encode_exec_tx_rollback_payload,
    map_exec_wal_evidence_to_tx_replay,
};
