use andromeda_catalog::CatalogSnapshot;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{
    CommitVisibleTrace, EventCorrelation, EventEmitter, EventSink, RollbackDurableTrace,
    TraceEvent, TraceId,
};
use andromeda_quic::SurfacePlane;
use andromeda_tx::TransactionManager;

use crate::{
    AuthorizedProcedureDispatch, ExecutionIoAdmissionDecision, InvocationContext, InvocationReject,
    InvocationRequest, InvocationWal, LocalDispatchPlan, LocalDispatcher, LocalRollbackPlan,
    RollbackCause, services::CompletionMappingService,
};

use super::helpers::{require_local_procedure_execution_io_admission, rollback_payload_for_cause};
use super::types::{LocalProcedure, VerticalInvocationOutcome};

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

    pub fn execute(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.execute_after_admission(request, procedure, trace_id, None)
    }

    pub fn execute_authorized(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.execute_after_admission(request, procedure, context.trace_id, Some(context))
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

        self.execute_authorized(request, procedure, context)
    }

    pub fn execute_catalog_resolved(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        catalog: &CatalogSnapshot,
        trace_id: TraceId,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.validate_catalog_resolved_procedure(&request, procedure, catalog, trace_id)?;
        self.execute_after_admission(request, procedure, trace_id, None)
    }

    pub fn execute_authorized_catalog_resolved(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        catalog: &CatalogSnapshot,
        context: &InvocationContext,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        self.validate_catalog_resolved_procedure(&request, procedure, catalog, context.trace_id)?;
        self.execute_after_admission(request, procedure, context.trace_id, Some(context))
    }

    pub fn execute_io_admitted(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let _io_admission = require_local_procedure_execution_io_admission(io_admission)?;
        self.execute_after_admission(request, procedure, trace_id, None)
    }

    pub fn execute_authorized_io_admitted(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        context: &InvocationContext,
        io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        let _io_admission = require_local_procedure_execution_io_admission(io_admission)?;
        self.execute_after_admission(request, procedure, context.trace_id, Some(context))
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
        let outcome = self.execute_authorized(request, procedure, context)?;

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
    ) -> AndromedaResult<VerticalInvocationOutcome> {
        procedure.validate()?;
        let admission_trace = request
            .validate_admission(trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
        let contract_trace = request
            .validate_before_transaction(procedure.contract, trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
        let authorization_trace = context
            .map(|context| {
                context
                    .authorize(&procedure.required_permissions)
                    .map_err(|reject| {
                        AndromedaError::new(AndromedaErrorKind::Security, reject.reason)
                    })
            })
            .transpose()?;

        // Allocate a recovery-safe transaction id from the runtime-owned
        // TransactionManager. The manager mirrors the InFlight status into
        // its status table so MVCC visibility queries see the transaction
        // before the dispatcher reaches the durable WAL flush.
        let transaction_id = self.transactions.begin()?;

        let dispatch_receipt =
            LocalDispatcher::new(&mut self.wal).dispatch_commit(LocalDispatchPlan {
                transaction_id,
                mutation_payload: procedure.mutation_payload.clone(),
                rows_affected: procedure.rows_affected,
            })?;

        // Mirror the durable commit into the manager only after the
        // dispatcher returns evidence that the WAL flush succeeded. The
        // status table publishes Committed strictly after that durable LSN
        // is known, preserving the "visible commit requires durable WAL
        // evidence" doctrine.
        self.transactions.request_commit(transaction_id)?;
        self.transactions
            .commit_durable(transaction_id, dispatch_receipt.durable_lsn.get())?;
        self.transactions.dispose(transaction_id)?;

        let completion = CompletionMappingService::committed(
            request.invocation_id,
            dispatch_receipt.rows_affected,
            dispatch_receipt.transaction_state,
            dispatch_receipt.durable_lsn,
            trace_id,
        )?;

        Ok(VerticalInvocationOutcome {
            completion,
            transaction_id,
            admission_trace,
            contract_trace,
            authorization_trace,
            result_metadata: procedure.result_metadata,
        })
    }

    fn validate_catalog_resolved_procedure(
        &self,
        request: &InvocationRequest,
        procedure: &LocalProcedure,
        catalog: &CatalogSnapshot,
        trace_id: TraceId,
    ) -> AndromedaResult<()> {
        request
            .validate_admission(trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;

        if request.catalog_version != catalog.version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "CatalogVersion mismatch against visible catalog snapshot before transaction creation",
            ));
        }

        let published_contract = catalog
            .get_procedure_by_id(request.procedure.procedure_id)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure is absent from visible catalog snapshot before transaction creation",
                )
            })?;

        if !catalog.is_active_object(published_contract.object.object_id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure is not active in visible catalog snapshot before transaction creation",
            ));
        }

        request
            .validate_before_transaction(published_contract.as_ref(), trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;

        if procedure.contract != published_contract.as_ref() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "local executable procedure does not match visible catalog contract before transaction creation",
            ));
        }

        Ok(())
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
        procedure.validate()?;
        let admission_trace = request
            .validate_admission(trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
        let contract_trace = request
            .validate_before_transaction(procedure.contract, trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
        let authorization_trace = context
            .map(|context| {
                context
                    .authorize(&procedure.required_permissions)
                    .map_err(|reject| {
                        AndromedaError::new(AndromedaErrorKind::Security, reject.reason)
                    })
            })
            .transpose()?;

        let rollback_payload = rollback_payload_for_cause(cause, &failure_reason)?;
        // Allocate a recovery-safe transaction id and route the manager
        // through the cause-implied intermediate state before dispatching
        // the WAL rollback. The manager state machine refuses any other
        // ordering, which keeps the TransactionStatus mirror in lockstep
        // with the dispatcher's local state machine.
        let transaction_id = self.transactions.begin()?;
        match cause {
            RollbackCause::Direct => {}
            RollbackCause::BusinessFailure => {
                self.transactions.fail(transaction_id)?;
            }
            RollbackCause::Poison => {
                self.transactions.poison(transaction_id)?;
            }
        }

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
        self.transactions.request_rollback(transaction_id)?;
        self.transactions
            .rollback_durable(transaction_id, rollback_receipt.durable_lsn.get())?;
        self.transactions.dispose(transaction_id)?;

        let completion = CompletionMappingService::rolled_back(
            request.invocation_id,
            rollback_receipt.transaction_state,
            rollback_receipt.durable_lsn,
            trace_id,
        )?;

        Ok(VerticalInvocationOutcome {
            completion,
            transaction_id,
            admission_trace,
            contract_trace,
            authorization_trace,
            result_metadata: procedure.result_metadata,
        })
    }

    /// Emit a CommitVisible event for a successfully committed transaction.
    /// This is a C5 observable coverage helper that emits the durable commit
    /// decision into the event sink. Event emission failures propagate to the
    /// caller (never silently dropped).
    pub fn emit_commit_visible_event<S: EventSink>(
        emitter: &mut EventEmitter<S>,
        trace_id: TraceId,
        transaction_id: andromeda_core::TransactionId,
        durable_lsn: andromeda_storage::Lsn,
        correlation: EventCorrelation,
    ) -> AndromedaResult<()> {
        emitter.emit(
            correlation,
            TraceEvent::CommitVisible(CommitVisibleTrace {
                trace_id,
                transaction_id,
                durable_commit_lsn: durable_lsn.get(),
            }),
        )?;
        Ok(())
    }

    /// Emit a RollbackDurable event for a successfully rolled-back transaction.
    /// This is a C5 observable coverage helper that emits the durable rollback
    /// decision into the event sink, capturing the allocator floor as visible
    /// state for recovery. Event emission failures propagate to the caller
    /// (never silently dropped).
    pub fn emit_rollback_durable_event<S: EventSink>(
        emitter: &mut EventEmitter<S>,
        trace_id: TraceId,
        transaction_id: andromeda_core::TransactionId,
        durable_lsn: andromeda_storage::Lsn,
        correlation: EventCorrelation,
    ) -> AndromedaResult<()> {
        emitter.emit(
            correlation,
            TraceEvent::RollbackDurable(RollbackDurableTrace {
                trace_id,
                transaction_id,
                durable_rollback_lsn: durable_lsn.get(),
            }),
        )?;
        Ok(())
    }
}
