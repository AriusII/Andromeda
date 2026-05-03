use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{DecisionTrace, TraceId};

use crate::{
    services::CompletionMappingService, transaction_id_for_invocation, InvocationCompletion, InvocationContext, InvocationRequest,
    InvocationWal, LocalDispatchPlan, LocalDispatcher,
    ResultStreamMetadata,
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

        Ok(VerticalInvocationOutcome {
            completion: CompletionMappingService::committed(
                request.invocation_id,
                dispatch_receipt.rows_affected,
                dispatch_receipt.transaction_state,
                dispatch_receipt.durable_lsn,
                trace_id,
            ),
            admission_trace,
            contract_trace,
            authorization_trace,
            result_metadata: procedure.result_metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{inventory_reserve_stock_contract, ProcedureContractRef};
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
