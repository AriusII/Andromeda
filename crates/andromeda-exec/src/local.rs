use andromeda_catalog::{CatalogSnapshot, ProcedureContractRef};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, PipelineClass, TransactionId,
};
use andromeda_observe::{DecisionTrace, TraceId};
use andromeda_quic::SurfacePlane;
use andromeda_tx::TransactionManager;

use crate::{
    services::CompletionMappingService, AuthorizedProcedureDispatch, CompletionStatus,
    ExecutionIoAdmissionDecision, InvocationCompletion, InvocationContext, InvocationReject,
    InvocationRequest, InvocationWal, LocalDispatchPlan, LocalDispatcher, LocalRollbackPlan,
    ResultStreamMetadata, RollbackCause,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProcedure {
    pub contract: ProcedureContractRef,
    pub required_permissions: Vec<String>,
    pub result_metadata: ResultStreamMetadata,
    pub mutation_payload: Vec<u8>,
    pub rows_affected: u64,
}

impl LocalProcedure {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract.validate()?;
        self.result_metadata.validate_before_payload()?;

        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "required permission must not be empty",
                ));
            }
        }

        if self.mutation_payload.is_empty() && self.rows_affected != 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "mutation payload must exist when rows are affected",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerticalInvocationOutcome {
    pub completion: InvocationCompletion,
    /// Recovery-safe transaction id allocated by the runtime's
    /// [`TransactionManager`] for this invocation. This is the authoritative
    /// id stamped on every WAL record and result-stream frame; downstream
    /// emitters must read it from here rather than re-deriving it from
    /// [`InvocationCompletion::invocation_id`].
    pub transaction_id: TransactionId,
    pub admission_trace: DecisionTrace,
    pub contract_trace: DecisionTrace,
    pub authorization_trace: Option<DecisionTrace>,
    pub result_metadata: ResultStreamMetadata,
}

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
}

pub fn require_local_procedure_execution_io_admission(
    io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
) -> AndromedaResult<ExecutionIoAdmissionDecision> {
    let decision = io_admission.map_err(|reject| {
        AndromedaError::new(
            io_admission_error_kind(reject.status),
            format!(
                "execution IO admission rejected before local procedure execution: {}",
                reject.reason
            ),
        )
    })?;

    if decision.pipeline_class != PipelineClass::ForegroundExecution {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!(
                "execution IO admission decision for local procedure execution must target {} pipeline, got {}",
                PipelineClass::ForegroundExecution.name(),
                decision.pipeline_class.name()
            ),
        ));
    }

    if !decision.trace.has_explanation() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "execution IO admission decision for local procedure execution must include an explicit reason",
        ));
    }

    Ok(decision)
}

fn io_admission_error_kind(status: CompletionStatus) -> AndromedaErrorKind {
    match status {
        CompletionStatus::PermissionDenied => AndromedaErrorKind::Security,
        CompletionStatus::ContractRejected => AndromedaErrorKind::Contract,
        CompletionStatus::SystemUnavailable => AndromedaErrorKind::Resource,
        CompletionStatus::Committed
        | CompletionStatus::RolledBack
        | CompletionStatus::FailedBeforeTransaction
        | CompletionStatus::Cancelled
        | CompletionStatus::Poisoned => AndromedaErrorKind::Execution,
    }
}

fn rollback_payload_for_cause(cause: RollbackCause, reason: &str) -> AndromedaResult<Vec<u8>> {
    let trimmed_reason = reason.trim();
    if trimmed_reason.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            match cause {
                RollbackCause::Direct | RollbackCause::BusinessFailure => {
                    "business validation rollback reason must not be empty"
                }
                RollbackCause::Poison => "poison rollback reason must not be empty",
            },
        ));
    }

    let domain: &[u8] = match cause {
        // Preserve the historical business-validation domain tag so existing
        // recovery tools and tests keep parsing the WAL payload format.
        RollbackCause::Direct | RollbackCause::BusinessFailure => {
            b"andromeda.exec.business-validation-failed.v1"
        }
        RollbackCause::Poison => b"andromeda.exec.poisoned-rollback.v1",
    };
    let mut payload = Vec::with_capacity(domain.len() + 1 + trimmed_reason.len());
    payload.extend_from_slice(domain);
    payload.push(0);
    payload.extend_from_slice(trimmed_reason.as_bytes());
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{
        inventory_domain_definition_batch, inventory_reserve_stock_contract,
        CatalogLifecycleTarget, CatalogSystemStore, DefinitionBatch, DefinitionBatchId,
        DefinitionOperation, ProcedureContract, ProcedureContractRef,
    };
    use andromeda_core::{
        CatalogVersion, ContractHash, DatabaseId, InvocationId, NamespaceId, ProcedureId,
        TransactionId,
    };
    use andromeda_observe::{
        AuthorizationDenialReason, AuthorizationOutcome, CertificateIdentity, Permission,
        PrincipalBinding, PrincipalRegistry, SecurityAuditOutcome, SurfaceScope, UserPrincipal,
        UserPrincipalKind,
    };
    use andromeda_srpl::Cardinality;
    use andromeda_storage::{InMemoryWal, Lsn, WalRecordKind};
    use andromeda_tx::TransactionState;

    use crate::{CompletionStatus, SurfacePlaneAuthorizer};

    #[derive(Debug, Default)]
    struct TestWal {
        records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
        durable_lsn: Lsn,
    }

    impl InvocationWal for TestWal {
        fn append(
            &mut self,
            kind: WalRecordKind,
            transaction_id: Option<TransactionId>,
            payload: &[u8],
        ) -> AndromedaResult<Lsn> {
            let lsn = Lsn::new(self.records.len() as u64 + 1);
            self.records
                .push((lsn, kind, transaction_id, payload.to_vec()));
            Ok(lsn)
        }

        fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn> {
            self.durable_lsn = lsn;
            Ok(lsn)
        }
    }

    fn request(expected_contract_hash: ContractHash) -> InvocationRequest {
        InvocationRequest {
            invocation_id: InvocationId::new(1),
            procedure: ProcedureContractRef {
                procedure_id: ProcedureId::new(2),
                contract_hash: ContractHash::test_vector(7),
                catalog_version: CatalogVersion::new(3),
            },
            expected_contract_hash,
            catalog_version: CatalogVersion::new(3),
            structured_parameters: Vec::new(),
        }
    }

    fn inventory_request(contract: &ProcedureContract) -> InvocationRequest {
        InvocationRequest {
            invocation_id: InvocationId::new(700),
            procedure: contract.as_ref(),
            expected_contract_hash: contract.contract_hash,
            catalog_version: contract.object.catalog_version,
            structured_parameters: Vec::new(),
        }
    }

    fn inventory_local_procedure(contract: &ProcedureContract) -> LocalProcedure {
        LocalProcedure {
            contract: contract.as_ref(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: contract.result_streams[0].columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"Inventory.ReserveStock".to_vec(),
            rows_affected: 1,
        }
    }

    fn inventory_catalog_store() -> CatalogSystemStore {
        let mut store = CatalogSystemStore::empty(
            DatabaseId::new(0x1000),
            NamespaceId::new(0x1001),
            CatalogVersion::new(0),
        );
        store
            .apply_definition_batch(&inventory_domain_definition_batch().unwrap())
            .unwrap();
        store
    }

    fn principal_registry(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
        let mut registry = PrincipalRegistry::new();
        for binding in bindings {
            registry.register(binding).unwrap();
        }
        registry
    }

    fn principal_binding(
        fingerprint: &str,
        surface: SurfaceScope,
        principal_id: &str,
        permissions: Vec<Permission>,
    ) -> PrincipalBinding {
        PrincipalBinding::new(
            CertificateIdentity::new(fingerprint, format!("CN={fingerprint}"), surface).unwrap(),
            UserPrincipal::new(principal_id, UserPrincipalKind::Service).unwrap(),
            permissions,
        )
        .unwrap()
    }

    #[test]
    fn local_vertical_runtime_commits_only_after_durable_wal() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = LocalProcedure {
            contract: request(ContractHash::test_vector(7)).procedure,
            required_permissions: Vec::new(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: 1,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"reserve-stock".to_vec(),
            rows_affected: 1,
        };

        let outcome = runtime
            .execute(
                request(ContractHash::test_vector(7)),
                &procedure,
                TraceId::new(99),
            )
            .unwrap();

        assert_eq!(outcome.completion.status, CompletionStatus::Committed);
        assert_eq!(
            outcome.completion.transaction_state,
            Some(TransactionState::Committed)
        );
        assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
        assert_eq!(runtime.wal().records.len(), 3);
        assert!(outcome.contract_trace.has_explanation());
    }

    #[test]
    fn local_vertical_runtime_rejects_contract_before_begin() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = LocalProcedure {
            contract: request(ContractHash::test_vector(7)).procedure,
            required_permissions: Vec::new(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: 1,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"reserve-stock".to_vec(),
            rows_affected: 1,
        };

        let err = runtime
            .execute(
                request(ContractHash::test_vector(8)),
                &procedure,
                TraceId::new(99),
            )
            .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(runtime.wal().records.is_empty());
    }

    #[test]
    fn local_vertical_runtime_rejects_executable_contract_mismatch_before_begin() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = LocalProcedure {
            contract: ProcedureContractRef {
                procedure_id: ProcedureId::new(99),
                ..request(ContractHash::test_vector(7)).procedure
            },
            required_permissions: Vec::new(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: 1,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"reserve-stock".to_vec(),
            rows_affected: 1,
        };

        let err = runtime
            .execute(
                request(ContractHash::test_vector(7)),
                &procedure,
                TraceId::new(99),
            )
            .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(runtime.wal().records.is_empty());
    }

    #[test]
    fn local_vertical_runtime_uses_inventory_contract_and_in_memory_wal() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let request = InvocationRequest {
            invocation_id: InvocationId::new(700),
            procedure: contract.as_ref(),
            expected_contract_hash: contract.contract_hash,
            catalog_version: contract.object.catalog_version,
            structured_parameters: Vec::new(),
        };
        let procedure = LocalProcedure {
            contract: contract.as_ref(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: contract.result_streams[0].columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"Inventory.ReserveStock".to_vec(),
            rows_affected: 1,
        };
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let outcome = runtime
            .execute_authorized(
                request,
                &procedure,
                &InvocationContext::new(TraceId::new(7000), contract.required_permissions.clone()),
            )
            .unwrap();

        assert_eq!(outcome.completion.status, CompletionStatus::Committed);
        assert!(outcome.authorization_trace.is_some());
        assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
        assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
        assert_eq!(runtime.wal().replay_durable().len(), 3);
    }

    #[test]
    fn local_vertical_runtime_rejects_missing_permission_before_begin() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let request = InvocationRequest {
            invocation_id: InvocationId::new(701),
            procedure: contract.as_ref(),
            expected_contract_hash: contract.contract_hash,
            catalog_version: contract.object.catalog_version,
            structured_parameters: Vec::new(),
        };
        let procedure = LocalProcedure {
            contract: contract.as_ref(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: contract.result_streams[0].columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"Inventory.ReserveStock".to_vec(),
            rows_affected: 1,
        };
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let err = runtime
            .execute_authorized(
                request,
                &procedure,
                &InvocationContext::new(TraceId::new(7001), Vec::new()),
            )
            .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Security);
        assert!(runtime.wal().is_empty());
    }

    #[test]
    fn surface_dispatch_denial_stops_before_transaction_creation() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let context =
            InvocationContext::new(TraceId::new(7002), contract.required_permissions.clone());
        let registry = principal_registry(Vec::new());
        let gate = SurfacePlaneAuthorizer::new(&registry);
        let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let denied = gate
            .authorize_procedure_dispatch(
                context.trace_id,
                SurfacePlane::Application,
                "fp-not-registered",
            )
            .unwrap()
            .unwrap_err();

        assert!(denied.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = denied {
            assert_eq!(reason, AuthorizationDenialReason::UnknownCertificate);
            assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
            assert!(audit.has_identity_evidence());
        }
        assert!(runtime.wal().is_empty());
        assert_eq!(runtime.transactions().live_count(), 0);
    }

    #[test]
    fn surface_authorized_dispatch_token_executes_through_local_runtime() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let procedure = inventory_local_procedure(&contract);
        let context =
            InvocationContext::new(TraceId::new(7003), contract.required_permissions.clone());
        let registry = principal_registry(vec![principal_binding(
            "fp-app-dispatch",
            SurfaceScope::Application,
            "svc-dispatch",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&registry);
        let token = gate
            .authorize_procedure_dispatch(
                context.trace_id,
                SurfacePlane::Application,
                "fp-app-dispatch",
            )
            .unwrap()
            .unwrap();
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let outcome = runtime
            .execute_surface_authorized(inventory_request(&contract), &procedure, &context, &token)
            .unwrap();

        assert_eq!(outcome.completion.status, CompletionStatus::Committed);
        assert_eq!(token.audit().outcome, SecurityAuditOutcome::Allowed);
        assert_eq!(runtime.wal().replay_durable().len(), 3);
    }

    #[test]
    fn administration_surface_cannot_present_procedure_dispatch_as_external_runtime_path() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let registry = principal_registry(vec![principal_binding(
            "fp-admin-with-exec",
            SurfaceScope::Administration,
            "ops-dispatch",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&registry);
        let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let denied = gate
            .authorize_procedure_dispatch(
                TraceId::new(7004),
                SurfacePlane::Administration,
                "fp-admin-with-exec",
            )
            .unwrap()
            .unwrap_err();

        assert!(denied.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = denied {
            assert_eq!(
                reason,
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission
            );
            assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
            assert!(audit.reason.contains("requires_application_surface"));
        }
        assert!(runtime.wal().is_empty());
        assert_eq!(runtime.transactions().live_count(), 0);
        assert!(contract.required_permissions.iter().all(|p| !p.is_empty()));
    }

    #[test]
    fn catalog_backed_runtime_resolves_visible_procedure_before_begin() {
        let store = inventory_catalog_store();
        let contract = inventory_reserve_stock_contract().unwrap();
        let procedure = inventory_local_procedure(&contract);
        let context =
            InvocationContext::new(TraceId::new(7100), contract.required_permissions.clone());
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let outcome = runtime
            .execute_authorized_catalog_resolved(
                inventory_request(&contract),
                &procedure,
                store.snapshot(),
                &context,
            )
            .unwrap();

        assert_eq!(outcome.completion.status, CompletionStatus::Committed);
        assert_eq!(runtime.wal().replay_durable().len(), 3);
    }

    #[test]
    fn catalog_backed_runtime_rejects_absent_procedure_before_begin() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let procedure = inventory_local_procedure(&contract);
        let catalog = CatalogSnapshot::empty(
            DatabaseId::new(0x1000),
            NamespaceId::new(0x1001),
            CatalogVersion::new(1),
        );
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let error = runtime
            .execute_catalog_resolved(
                inventory_request(&contract),
                &procedure,
                &catalog,
                TraceId::new(7101),
            )
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("absent"));
        assert!(runtime.wal().is_empty());
    }

    #[test]
    fn catalog_backed_runtime_rejects_inactive_procedure_before_begin() {
        let mut store = inventory_catalog_store();
        let contract = inventory_reserve_stock_contract().unwrap();
        let procedure = inventory_local_procedure(&contract);
        store
            .apply_definition_batch(&DefinitionBatch {
                batch_id: DefinitionBatchId::new(0x7102),
                database_id: DatabaseId::new(0x1000),
                namespace_id: NamespaceId::new(0x1001),
                base_version: CatalogVersion::new(1),
                operations: vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: contract.object.clone(),
                })],
            })
            .unwrap();
        let mut request = inventory_request(&contract);
        request.catalog_version = store.snapshot().version;
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let error = runtime
            .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7102))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("not active"));
        assert!(runtime.wal().is_empty());
    }

    #[test]
    fn catalog_backed_runtime_rejects_stale_catalog_version_before_begin() {
        let store = inventory_catalog_store();
        let contract = inventory_reserve_stock_contract().unwrap();
        let procedure = inventory_local_procedure(&contract);
        let mut request = inventory_request(&contract);
        request.catalog_version = CatalogVersion::new(2);
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let error = runtime
            .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7103))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("CatalogVersion mismatch"));
        assert!(runtime.wal().is_empty());
    }

    #[test]
    fn catalog_backed_runtime_rejects_contract_hash_mismatch_before_begin() {
        let store = inventory_catalog_store();
        let contract = inventory_reserve_stock_contract().unwrap();
        let procedure = inventory_local_procedure(&contract);
        let mut request = inventory_request(&contract);
        request.expected_contract_hash = ContractHash::test_vector(0x99);
        let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

        let error = runtime
            .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7104))
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("ContractHash mismatch"));
        assert!(runtime.wal().is_empty());
    }

    // --- failure routing through Failed/Poisoned before durable rollback ---

    fn failure_request() -> InvocationRequest {
        request(ContractHash::test_vector(7))
    }

    fn failure_procedure() -> LocalProcedure {
        LocalProcedure {
            contract: failure_request().procedure,
            required_permissions: Vec::new(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: 1,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"reserve-stock".to_vec(),
            rows_affected: 1,
        }
    }

    #[test]
    fn business_failure_routes_through_failed_before_durable_rollback() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = failure_procedure();

        let outcome = runtime
            .rollback_business_validation_failure_after_begin(
                failure_request(),
                &procedure,
                TraceId::new(9100),
                "insufficient inventory stock for reservation",
            )
            .unwrap();

        // No success-shaped completion — RolledBack only after durable evidence.
        assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
        assert_eq!(
            outcome.completion.transaction_state,
            Some(TransactionState::RolledBack)
        );
        assert_eq!(outcome.completion.rows_affected, Some(0));
        assert!(outcome.completion.durable_lsn.is_some());

        // WAL evidence: TxBegin then TxRollback (no TxCommit), and durable
        // flush must reach the rollback LSN.
        let wal = runtime.wal();
        assert_eq!(wal.records.len(), 2);
        assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
        assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
        assert!(!wal
            .records
            .iter()
            .any(|(_, kind, _, _)| *kind == WalRecordKind::TxCommit));
        assert!(wal.durable_lsn >= wal.records[1].0);
        assert!(wal.records[1]
            .3
            .starts_with(b"andromeda.exec.business-validation-failed.v1\0"));
    }

    #[test]
    fn poison_failure_routes_through_poisoned_before_durable_rollback() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = failure_procedure();

        let outcome = runtime
            .rollback_poison_after_begin(
                failure_request(),
                &procedure,
                TraceId::new(9101),
                "engine invariant violated by post-begin runtime witness",
            )
            .unwrap();

        assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
        assert_eq!(
            outcome.completion.transaction_state,
            Some(TransactionState::RolledBack)
        );
        assert_eq!(outcome.completion.rows_affected, Some(0));
        assert!(outcome.completion.durable_lsn.is_some());

        let wal = runtime.wal();
        assert_eq!(wal.records.len(), 2);
        assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
        assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
        assert!(!wal
            .records
            .iter()
            .any(|(_, kind, _, _)| *kind == WalRecordKind::TxCommit));
        assert!(wal.durable_lsn >= wal.records[1].0);
        assert!(wal.records[1]
            .3
            .starts_with(b"andromeda.exec.poisoned-rollback.v1\0"));
    }

    #[test]
    fn dispatcher_rollback_with_cause_records_intermediate_state() {
        use crate::RollbackCause;

        for (cause, expected_intermediate) in [
            (RollbackCause::Direct, None),
            (
                RollbackCause::BusinessFailure,
                Some(TransactionState::Failed),
            ),
            (RollbackCause::Poison, Some(TransactionState::Poisoned)),
        ] {
            let mut wal = TestWal::default();
            let receipt = LocalDispatcher::new(&mut wal)
                .dispatch_rollback_with_cause(
                    LocalRollbackPlan {
                        transaction_id: TransactionId::new(0xDEAD_BEEF),
                        rollback_payload: b"cause-routing".to_vec(),
                    },
                    cause,
                )
                .unwrap();

            assert_eq!(receipt.cause, cause);
            assert_eq!(receipt.intermediate_state, expected_intermediate);
            // RolledBack only after durable rollback flush regardless of cause.
            assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
            assert!(receipt.durable_lsn >= receipt.wal_evidence.rollback_lsn);
        }
    }

    #[test]
    fn poison_rollback_rejects_empty_reason() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = failure_procedure();

        let err = runtime
            .rollback_poison_after_begin(failure_request(), &procedure, TraceId::new(9102), "  ")
            .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Execution);
        assert!(runtime.wal().records.is_empty());
    }
}
