use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, PipelineClass};
use andromeda_observe::{DecisionTrace, TraceId};

use crate::{
    CompletionStatus, ExecutionIoAdmissionDecision, InvocationCompletion, InvocationContext,
    InvocationReject, InvocationRequest, InvocationWal, LocalDispatchPlan, LocalDispatcher,
    LocalRollbackPlan, ResultStreamMetadata, services::CompletionMappingService,
    transaction_id_for_invocation,
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
    pub admission_trace: DecisionTrace,
    pub contract_trace: DecisionTrace,
    pub authorization_trace: Option<DecisionTrace>,
    pub result_metadata: ResultStreamMetadata,
}

pub struct LocalVerticalRuntime<W> {
    wal: W,
}

impl<W> LocalVerticalRuntime<W>
where
    W: InvocationWal,
{
    pub const fn new(wal: W) -> Self {
        Self { wal }
    }

    pub fn wal(&self) -> &W {
        &self.wal
    }

    pub fn wal_mut(&mut self) -> &mut W {
        &mut self.wal
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
        self.rollback_business_validation_failure_after_admission(
            request,
            procedure,
            trace_id,
            None,
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
        self.rollback_business_validation_failure_after_admission(
            request,
            procedure,
            context.trace_id,
            Some(context),
            failure_reason.into(),
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

        let dispatch_receipt =
            LocalDispatcher::new(&mut self.wal).dispatch_commit(LocalDispatchPlan {
                transaction_id: transaction_id_for_invocation(request.invocation_id),
                mutation_payload: procedure.mutation_payload.clone(),
                rows_affected: procedure.rows_affected,
            })?;

        let completion = CompletionMappingService::committed(
            request.invocation_id,
            dispatch_receipt.rows_affected,
            dispatch_receipt.transaction_state,
            dispatch_receipt.durable_lsn,
            trace_id,
        )?;

        Ok(VerticalInvocationOutcome {
            completion,
            admission_trace,
            contract_trace,
            authorization_trace,
            result_metadata: procedure.result_metadata,
        })
    }

    fn rollback_business_validation_failure_after_admission(
        &mut self,
        request: InvocationRequest,
        procedure: &LocalProcedure,
        trace_id: TraceId,
        context: Option<&InvocationContext>,
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

        let rollback_payload = rollback_payload_for_business_validation_failure(&failure_reason)?;
        let rollback_receipt =
            LocalDispatcher::new(&mut self.wal).dispatch_rollback(LocalRollbackPlan {
                transaction_id: transaction_id_for_invocation(request.invocation_id),
                rollback_payload,
            })?;

        let completion = CompletionMappingService::rolled_back(
            request.invocation_id,
            rollback_receipt.transaction_state,
            rollback_receipt.durable_lsn,
            trace_id,
        )?;

        Ok(VerticalInvocationOutcome {
            completion,
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

fn rollback_payload_for_business_validation_failure(reason: &str) -> AndromedaResult<Vec<u8>> {
    let trimmed_reason = reason.trim();
    if trimmed_reason.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "business validation rollback reason must not be empty",
        ));
    }

    let domain = b"andromeda.exec.business-validation-failed.v1";
    let mut payload = Vec::with_capacity(domain.len() + 1 + trimmed_reason.len());
    payload.extend_from_slice(domain);
    payload.push(0);
    payload.extend_from_slice(trimmed_reason.as_bytes());
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{ProcedureContractRef, inventory_reserve_stock_contract};
    use andromeda_core::{CatalogVersion, ContractHash, InvocationId, ProcedureId, TransactionId};
    use andromeda_srpl::Cardinality;
    use andromeda_storage::{InMemoryWal, Lsn, WalRecordKind};
    use andromeda_tx::TransactionState;

    use crate::CompletionStatus;

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

    #[test]
    fn local_vertical_runtime_commits_only_after_durable_wal() {
        let mut runtime = LocalVerticalRuntime::new(TestWal::default());
        let procedure = LocalProcedure {
            contract: request(ContractHash::test_vector(7)).procedure,
            required_permissions: Vec::new(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
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
}
