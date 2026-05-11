mod admission;
mod events;
mod records;
mod transactions;

use andromeda_catalog_store::CatalogSnapshot;
use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::{EventCorrelation, TraceId};
use andromeda_observe::{EventEmitter, EventSink};
use andromeda_quic::SurfacePlane;
use andromeda_transaction::TransactionManager;
use andromeda_wal::InvocationWal;

use crate::{
    AuthorizedProcedureDispatch, ExecutionIoAdmissionDecision, InvocationContext, InvocationReject,
    InvocationRequest, LocalDispatchPlan, LocalDispatcher, LocalRollbackPlan, RollbackCause,
    services::CompletionMappingService,
};

use super::helpers::{
    default_local_procedure_execution_io_admission,
    require_local_procedure_execution_io_admission_for_trace, rollback_payload_for_cause,
};
use super::types::{LocalProcedure, VerticalInvocationOutcome};
use admission::{
    validate_catalog_resolved_procedure, validate_pre_transaction_admission,
    validate_surface_dispatch_token,
};
use records::{RuntimeRecordSpan, committed_runtime_record, rolled_back_runtime_record};
use transactions::{
    begin_runtime_transaction, mark_commit_visible_after_durable_wal,
    mark_rollback_durable_after_wal, route_rollback_cause,
};

type CatalogPublicationReceipt = andromeda_catalog_store::CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

pub struct LocalVerticalRuntime<W> {
    wal: W,
    transactions: TransactionManager,
}

impl<W> LocalVerticalRuntime<W>
where
    W: InvocationWal,
{
    /// Construct a runtime with a freshly initialized
    /// [`TransactionManager`] (allocator floor = 0). Callers that perform
    /// recovery before serving traffic must instead use
    /// [`LocalVerticalRuntime::with_transaction_manager`] and seed the
    /// manager via [`TransactionManager::with_recovered_floor`] /
    /// [`TransactionManager::seed_allocator`] so post-restart ids stay
    /// strictly monotonic.
    pub fn new(wal: W) -> Self {
        Self {
            wal,
            transactions: TransactionManager::new(),
        }
    }

    /// Construct a runtime that shares an externally owned, possibly
    /// recovery-seeded, [`TransactionManager`]. This is the recovery-safe
    /// entry point: the WAL replay driver builds the manager with the right
    /// allocator floor, then hands it to the runtime so the next allocated
    /// `TransactionId` is strictly greater than every id observed in the
    /// durable WAL.
    pub fn with_transaction_manager(wal: W, transactions: TransactionManager) -> Self {
        Self { wal, transactions }
    }

    pub fn wal(&self) -> &W {
        &self.wal
    }

    pub fn wal_mut(&mut self) -> &mut W {
        &mut self.wal
    }

    /// Borrow the runtime-owned [`TransactionManager`]. Exposed so the
    /// recovery driver and observability layers can inspect transaction
    /// status without re-deriving ids from invocation ids.
    pub fn transactions(&self) -> &TransactionManager {
        &self.transactions
    }

    /// Execute a locally resolved procedure without an authorization context.
    ///
    /// ⚠ **System-plane / internal use only.**
    /// Application-plane callers MUST use [`execute_surface_authorized()`] with
    /// a valid [`AuthorizedProcedureDispatch`] token. This method bypasses the
    /// surface gate and is intentionally left accessible only for test
    /// infrastructure, WAL replay, and engine-internal system procedures.
    pub fn execute_internal(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.execute_after_admission(request, procedure, trace_id, None, None)
    }

    /// Execute a locally resolved procedure with an authorization context but
    /// without a surface-gate dispatch token.
    ///
    /// ⚠ **System-plane / internal use only.**
    /// Application-plane callers MUST use [`execute_surface_authorized()`] with
    /// a valid [`AuthorizedProcedureDispatch`] token. This method bypasses the
    /// surface gate and is intentionally left accessible only for test
    /// infrastructure, WAL replay, and engine-internal system procedures.
    pub fn execute_internal_authorized(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.execute_after_admission(request, procedure, context.trace_id, Some(context), None)
    }

    /// Execute a locally resolved procedure only after the exec-owned
    /// transport-surface gate has authorized Application-plane procedure
    /// dispatch.
    ///
    /// This is the method external runtime dispatch adapters should use. The
    /// [`AuthorizedProcedureDispatch`] capability is intentionally only
    /// constructible by [`crate::SurfacePlaneAuthorizer`] after a typed
    /// mTLS/principal/surface/permission allow decision; if the gate returns a
    /// denial, callers must emit the denial audit and must not call into local
    /// execution, preserving the pre-transaction invariant.
    pub fn execute_surface_authorized(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        dispatch: &AuthorizedProcedureDispatch,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        if dispatch.plane() != SurfacePlane::Application {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure dispatch must be authorized on the Application surface before transaction creation",
            ));
        }
        validate_surface_dispatch_token(context, dispatch)?;

        self.execute_internal_authorized(request, procedure, context)
    }

    /// Execute a locally resolved procedure with catalog snapshot validation
    /// but without a surface-gate dispatch token.
    ///
    /// ⚠ **System-plane / internal use only.**
    /// Application-plane callers MUST use [`execute_surface_authorized()`] with
    /// a valid [`AuthorizedProcedureDispatch`] token. This method bypasses the
    /// surface gate and is intentionally left accessible only for test
    /// infrastructure, WAL replay, and engine-internal system procedures.
    pub fn execute_internal_catalog_resolved(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        catalog: &CatalogSnapshot<CatalogPublicationReceipt>,
        trace_id: TraceId,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        validate_catalog_resolved_procedure(&request, procedure, catalog, trace_id)?;
        self.execute_after_admission(request, procedure, trace_id, None, None)
    }

    pub fn execute_authorized_catalog_resolved(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        catalog: &CatalogSnapshot<CatalogPublicationReceipt>,
        context: &InvocationContext,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        validate_catalog_resolved_procedure(&request, procedure, catalog, context.trace_id)?;
        self.execute_after_admission(request, procedure, context.trace_id, Some(context), None)
    }

    pub fn execute_io_admitted(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.execute_after_admission(request, procedure, trace_id, None, Some(io_admission))
    }

    pub fn execute_authorized_io_admitted(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.execute_after_admission(
            request,
            procedure,
            context.trace_id,
            Some(context),
            Some(io_admission),
        )
    }

    /// Execute with observable event emission for commit lifecycle.
    /// C5 feature: emits CommitVisible event after successful commit flush.
    /// Event emission failures are NOT silently dropped and propagate to caller.
    pub fn execute_authorized_observable<S: EventSink>(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        emitter: &mut EventEmitter<S>,
        correlation: EventCorrelation,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let outcome = self.execute_internal_authorized(request, procedure, context)?;

        // Emit CommitVisible event after successful commit with durable LSN
        if let Some(durable_lsn) = outcome.completion.durable_lsn {
            Self::emit_commit_visible_event(
                emitter,
                outcome.completion.trace_id,
                outcome.transaction_id,
                durable_lsn,
                correlation,
            )?;
        }

        Ok(outcome)
    }

    /// Rollback with observable event emission for rollback lifecycle (business failure path).
    /// C5 feature: emits RollbackDurable event after successful rollback flush.
    /// Event emission failures are NOT silently dropped and propagate to caller.
    pub fn rollback_authorized_business_validation_failure_after_begin_observable<S: EventSink>(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        failure_reason: impl Into<String>,
        emitter: &mut EventEmitter<S>,
        correlation: EventCorrelation,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let outcome = self.rollback_authorized_business_validation_failure_after_begin(
            request,
            procedure,
            context,
            failure_reason,
        )?;

        // Emit RollbackDurable event after successful rollback with durable LSN
        if let Some(durable_lsn) = outcome.completion.durable_lsn {
            Self::emit_rollback_durable_event(
                emitter,
                outcome.completion.trace_id,
                outcome.transaction_id,
                durable_lsn,
                correlation,
            )?;
        }

        Ok(outcome)
    }

    /// Rollback with observable event emission for rollback lifecycle (poison path).
    /// C5 feature: emits RollbackDurable event after successful rollback flush.
    /// Event emission failures are NOT silently dropped and propagate to caller.
    pub fn rollback_authorized_poison_after_begin_observable<S: EventSink>(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        poison_reason: impl Into<String>,
        emitter: &mut EventEmitter<S>,
        correlation: EventCorrelation,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let outcome = self.rollback_authorized_poison_after_begin(
            request,
            procedure,
            context,
            poison_reason,
        )?;

        // Emit RollbackDurable event after successful rollback with durable LSN
        if let Some(durable_lsn) = outcome.completion.durable_lsn {
            Self::emit_rollback_durable_event(
                emitter,
                outcome.completion.trace_id,
                outcome.transaction_id,
                durable_lsn,
                correlation,
            )?;
        }

        Ok(outcome)
    }

    pub fn rollback_business_validation_failure_after_begin(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        failure_reason: impl Into<String>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.rollback_after_admission(
            request,
            procedure,
            trace_id,
            None,
            RollbackCause::BusinessFailure,
            failure_reason.into(),
        )
    }

    pub fn rollback_authorized_business_validation_failure_after_begin(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        failure_reason: impl Into<String>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.rollback_after_admission(
            request,
            procedure,
            context.trace_id,
            Some(context),
            RollbackCause::BusinessFailure,
            failure_reason.into(),
        )
    }

    /// Route a poisoned/rejected runtime path through `TransactionState::Poisoned`
    /// before driving a durable rollback and emitting a `RolledBack` completion.
    ///
    /// Use this when continued execution would violate engine invariants
    /// (e.g. catastrophic decoder rejection of an admitted command,
    /// MVCC visibility witnesses contradicting the snapshot the executor
    /// observed, or a runtime-level invariant breach detected after `Begin`).
    pub fn rollback_poison_after_begin(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        poison_reason: impl Into<String>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.rollback_after_admission(
            request,
            procedure,
            trace_id,
            None,
            RollbackCause::Poison,
            poison_reason.into(),
        )
    }

    pub fn rollback_authorized_poison_after_begin(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        poison_reason: impl Into<String>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.rollback_after_admission(
            request,
            procedure,
            context.trace_id,
            Some(context),
            RollbackCause::Poison,
            poison_reason.into(),
        )
    }

    fn execute_after_admission(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        context: Option<&InvocationContext>,
        io_admission: Option<Result<ExecutionIoAdmissionDecision, InvocationReject>>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let pre_transaction =
            validate_pre_transaction_admission(&request, procedure, trace_id, context)?;
        let _io_admission = match io_admission {
            Some(io_admission) => require_local_procedure_execution_io_admission_for_trace(
                io_admission,
                trace_id,
                request.procedure.procedure_id,
            )?,
            None => default_local_procedure_execution_io_admission(
                trace_id,
                request.procedure.procedure_id,
            )?,
        };
        let runtime_span = RuntimeRecordSpan::started();

        // Allocate a recovery-safe transaction id from the runtime-owned
        // TransactionManager. The manager mirrors the InFlight status into
        // its status table so MVCC visibility queries see the transaction
        // before the dispatcher reaches the durable WAL flush.
        let transaction_id = begin_runtime_transaction(&mut self.transactions)?;

        let dispatch_receipt =
            LocalDispatcher::new(&mut self.wal).dispatch_commit(LocalDispatchPlan {
                transaction_id,
                mutation_payload: procedure.mutation_payload.clone(),
                rows_affected: procedure.rows_affected,
            })?;

        // GAP-03: Validate that the handler's declared result metadata agrees
        // with the dispatcher's actual WAL and transaction evidence BEFORE we
        // publish the commit as MVCC-visible.
        //
        // This calls `validate_terminal_evidence` — NOT `validate_terminal_completion`
        // — because `dispatch_receipt.rows_affected` is the count of DB rows
        // mutated (e.g. 2 for ReserveStock: 1 stock row + 1 reservation row),
        // which is intentionally independent of `result_metadata.row_count_exact`
        // (the number of rows returned in the result stream, e.g. 1).
        // Cross-comparing them would produce spurious Contract errors on any
        // procedure that writes more DB rows than it returns.
        //
        // Checks enforced here:
        //   1. transaction_state is Committed or RolledBack.
        //   2. durable_lsn is non-zero (WAL is flushed).
        //   3. A RolledBack transaction reports zero DB mutations.
        //
        // NOTE: The WAL commit record is already durable at this point.
        // Failing here prevents MVCC visibility without undoing the WAL write
        // (WAL-before-commit doctrine). The transaction remains InFlight inside
        // the TransactionManager — not visible to MVCC readers.
        if let Err(validation_err) = procedure.result_metadata.validate_terminal_evidence(
            dispatch_receipt.transaction_state,
            dispatch_receipt.durable_lsn,
            dispatch_receipt.rows_affected,
        ) {
            // Attempt best-effort transaction cleanup to bound InFlight leak.
            let _ = route_rollback_cause(
                &mut self.transactions,
                transaction_id,
                RollbackCause::Poison,
            );
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "terminal evidence validation failed after durable WAL commit: {}",
                    validation_err.message()
                ),
            ));
        }

        // Mirror the durable commit into the manager only after the
        // dispatcher returns evidence that the WAL flush succeeded. The
        // status table publishes Committed strictly after that durable LSN
        // is known, preserving the "visible commit requires durable WAL
        // evidence" doctrine.
        mark_commit_visible_after_durable_wal(
            &mut self.transactions,
            transaction_id,
            dispatch_receipt.durable_lsn,
        )?;

        let completion = CompletionMappingService::committed(
            request.invocation_id,
            dispatch_receipt.rows_affected,
            dispatch_receipt.transaction_state,
            dispatch_receipt.durable_lsn,
            trace_id,
        )?;
        let runtime_record = committed_runtime_record(&request, procedure, runtime_span)?;

        Ok(VerticalInvocationOutcome {
            completion,
            transaction_id,
            admission_trace: pre_transaction.admission_trace,
            contract_trace: pre_transaction.contract_trace,
            authorization_trace: pre_transaction.authorization_trace,
            result_metadata: procedure.result_metadata,
            runtime_record,
            wal_evidence: Some(dispatch_receipt.wal_evidence),
        })
    }

    fn rollback_after_admission(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        context: Option<&InvocationContext>,
        cause: RollbackCause,
        failure_reason: String,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let pre_transaction =
            validate_pre_transaction_admission(&request, procedure, trace_id, context)?;
        let _io_admission = default_local_procedure_execution_io_admission(
            trace_id,
            request.procedure.procedure_id,
        )?;
        let runtime_span = RuntimeRecordSpan::started();

        let rollback_payload = rollback_payload_for_cause(cause, &failure_reason)?;
        let rollback_payload_len = rollback_payload.len();
        // Allocate a recovery-safe transaction id and route the manager
        // through the cause-implied intermediate state before dispatching
        // the WAL rollback. The manager state machine refuses any other
        // ordering, which keeps the TransactionStatus mirror in lockstep
        // with the dispatcher's local state machine.
        let transaction_id = begin_runtime_transaction(&mut self.transactions)?;
        route_rollback_cause(&mut self.transactions, transaction_id, cause)?;

        let rollback_receipt = LocalDispatcher::new(&mut self.wal).dispatch_rollback_with_cause(
            LocalRollbackPlan {
                transaction_id,
                rollback_payload,
            },
            cause,
        )?;

        // Defense in depth: if the dispatcher ever returned a receipt that did
        // not preserve the cause-implied intermediate state, refuse to emit a
        // rolled-back completion. This guarantees runtime business or poison
        // failures cannot be silently downgraded to a Direct rollback path.
        if rollback_receipt.cause != cause
            || rollback_receipt.intermediate_state != cause.intermediate_state()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rollback dispatcher did not honor the requested failure routing",
            ));
        }

        // Mirror the durable rollback into the manager strictly after the
        // dispatcher confirmed the WAL flush. Then dispose so the manager
        // stops tracking the live state-machine entry but the status table
        // retains the historical RolledBack outcome for visibility queries.
        mark_rollback_durable_after_wal(
            &mut self.transactions,
            transaction_id,
            rollback_receipt.durable_lsn,
        )?;

        let completion = CompletionMappingService::rolled_back(
            request.invocation_id,
            rollback_receipt.transaction_state,
            rollback_receipt.durable_lsn,
            trace_id,
        )?;
        let runtime_record = rolled_back_runtime_record(
            &request,
            procedure,
            cause,
            rollback_payload_len,
            runtime_span,
        )?;

        Ok(VerticalInvocationOutcome {
            completion,
            transaction_id,
            admission_trace: pre_transaction.admission_trace,
            contract_trace: pre_transaction.contract_trace,
            authorization_trace: pre_transaction.authorization_trace,
            result_metadata: procedure.result_metadata,
            runtime_record,
            wal_evidence: None,
        })
    }

    /// Emit a CommitVisible event for a successfully committed transaction.
    /// This is a C5 observable coverage helper that emits the durable commit
    /// decision into the event sink. Event emission failures propagate to the
    /// caller (never silently dropped).
    pub fn emit_commit_visible_event<S: EventSink>(
        emitter: &mut EventEmitter<S>,
        trace_id: TraceId,
        transaction_id: andromeda_types::TransactionId,
        durable_lsn: andromeda_wal::Lsn,
        correlation: EventCorrelation,
    ) -> AndromedaResult<()> {
        events::emit_commit_visible_event(
            emitter,
            trace_id,
            transaction_id,
            durable_lsn,
            correlation,
        )
    }

    /// Emit a RollbackDurable event for a successfully rolled-back transaction.
    /// This is a C5 observable coverage helper that emits the durable rollback
    /// decision into the event sink, capturing the allocator floor as visible
    /// state for recovery. Event emission failures propagate to the caller
    /// (never silently dropped).
    pub fn emit_rollback_durable_event<S: EventSink>(
        emitter: &mut EventEmitter<S>,
        trace_id: TraceId,
        transaction_id: andromeda_types::TransactionId,
        durable_lsn: andromeda_wal::Lsn,
        correlation: EventCorrelation,
    ) -> AndromedaResult<()> {
        events::emit_rollback_durable_event(
            emitter,
            trace_id,
            transaction_id,
            durable_lsn,
            correlation,
        )
    }

    /// Execute a locally resolved procedure with full P04 invocation trace
    /// emission.
    ///
    /// Runs the complete commit pipeline (identical to
    /// [`execute_authorized_observable`][Self::execute_authorized_observable])
    /// and additionally:
    ///
    /// 1. Emits 8 `ExecutionTransitionTrace` events covering the ordered
    ///    lifecycle phases (ADMITTED → CONTRACT_BOUND → PERMISSION_CHECKED →
    ///    BUDGET_RESERVED → TRANSACTION_OPENED → EXECUTING → COMMITTING →
    ///    COMMITTED).
    /// 2. Appends one [`andromeda_execution_trace::ProcedureInvocationTrace`]
    ///    record to `ledger` with the terminal [`CompletionStatus`].
    ///
    /// Event and ledger failures propagate to the caller (never silently
    /// dropped).
    pub fn execute_authorized_with_invocation_trace<S: EventSink>(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        emitter: &mut EventEmitter<S>,
        correlation: EventCorrelation,
        ledger: &dyn andromeda_execution_trace::AuditLedger,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let invocation_id = request.invocation_id;
        let trace_id = context.trace_id;

        let inv_trace =
            andromeda_execution_trace::ProcedureInvocationTrace::started(trace_id, invocation_id);

        let outcome = self.execute_internal_authorized(request, procedure, context)?;

        let transaction_id = outcome.transaction_id;
        // Committed completions carry a durable_lsn; unwrap it for the
        // terminal COMMITTED transition.
        let durable_lsn = outcome.completion.durable_lsn.map(|l| l.get()).unwrap_or(0);

        events::emit_invocation_commit_transition_sequence(
            emitter,
            trace_id,
            invocation_id,
            transaction_id,
            durable_lsn,
            correlation,
        )?;

        let completion_status = outcome.completion.status;
        let inv_trace = inv_trace.complete(completion_status);
        inv_trace.validate()?;
        ledger.append_procedure_trace(inv_trace)?;

        Ok(outcome)
    }

    /// Roll back a business-failure invocation with full P04 invocation trace
    /// emission.
    ///
    /// Runs the business-failure rollback pipeline (identical to
    /// [`rollback_authorized_business_validation_failure_after_begin_observable`][Self::rollback_authorized_business_validation_failure_after_begin_observable])
    /// and additionally:
    ///
    /// 1. Emits 8 `ExecutionTransitionTrace` events covering the ordered
    ///    lifecycle phases ending in ROLLED_BACK.
    /// 2. Appends one [`andromeda_execution_trace::ProcedureInvocationTrace`]
    ///    record to `ledger` with the terminal [`CompletionStatus`].
    ///
    /// Event and ledger failures propagate to the caller (never silently
    /// dropped).
    #[allow(clippy::too_many_arguments)]
    pub fn rollback_authorized_business_validation_failure_after_begin_with_invocation_trace<
        S: EventSink,
    >(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        failure_reason: impl Into<String>,
        emitter: &mut EventEmitter<S>,
        correlation: EventCorrelation,
        ledger: &dyn andromeda_execution_trace::AuditLedger,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let invocation_id = request.invocation_id;
        let trace_id = context.trace_id;

        let inv_trace =
            andromeda_execution_trace::ProcedureInvocationTrace::started(trace_id, invocation_id);

        let outcome = self.rollback_authorized_business_validation_failure_after_begin(
            request,
            procedure,
            context,
            failure_reason,
        )?;

        let transaction_id = outcome.transaction_id;
        let durable_lsn = outcome.completion.durable_lsn.map(|l| l.get()).unwrap_or(0);

        events::emit_invocation_rollback_transition_sequence(
            emitter,
            trace_id,
            invocation_id,
            transaction_id,
            durable_lsn,
            correlation,
        )?;

        let completion_status = outcome.completion.status;
        let inv_trace = inv_trace.complete(completion_status);
        inv_trace.validate()?;
        ledger.append_procedure_trace(inv_trace)?;

        Ok(outcome)
    }
}
