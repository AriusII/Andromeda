use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_observe::{DecisionTrace, TraceId};
use andromeda_storage::WalRecordKind;
use andromeda_tx::{TransactionEvent, TransactionStateMachine};

use crate::{
    CompletionStatus, InvocationCompletion, InvocationRequest, InvocationWal, ResultStreamMetadata,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProcedure {
    pub contract: ProcedureContractRef,
    pub result_metadata: ResultStreamMetadata,
    pub mutation_payload: Vec<u8>,
    pub rows_affected: u64,
}

impl LocalProcedure {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract.validate()?;
        self.result_metadata.validate_before_payload()?;

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
    pub contract_trace: DecisionTrace,
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
        procedure.validate()?;
        let contract_trace = request
            .validate_before_transaction(trace_id)
            .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;

        let tx_id = TransactionId::new(request.invocation_id.get());
        let mut tx = TransactionStateMachine::new(tx_id);
        tx.apply(TransactionEvent::Begin)?;

        self.wal
            .append(WalRecordKind::TxBegin, Some(tx_id), b"tx-begin")?;

        if procedure.rows_affected > 0 {
            self.wal.append(
                WalRecordKind::RowUpdate,
                Some(tx_id),
                &procedure.mutation_payload,
            )?;
        }

        tx.apply(TransactionEvent::CommitRequested)?;
        let commit_lsn = self
            .wal
            .append(WalRecordKind::TxCommit, Some(tx_id), b"tx-commit")?;
        let durable_lsn = self.wal.flush_through(commit_lsn)?;
        tx.mark_durable_commit_lsn(durable_lsn.get())?;
        tx.apply(TransactionEvent::DurableWalFlushed)?;

        Ok(VerticalInvocationOutcome {
            completion: InvocationCompletion {
                invocation_id: request.invocation_id,
                status: CompletionStatus::Committed,
                rows_affected: Some(procedure.rows_affected),
                transaction_state: Some(tx.state),
                durable_lsn: Some(durable_lsn),
                trace_id,
            },
            contract_trace,
            result_metadata: procedure.result_metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{inventory_reserve_stock_contract, ProcedureContractRef};
    use andromeda_core::{CatalogVersion, ContractHash, InvocationId, ProcedureId};
    use andromeda_srpl::Cardinality;
    use andromeda_storage::{InMemoryWal, Lsn, WalRecordKind};
    use andromeda_tx::TransactionState;

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
            .execute(request, &procedure, TraceId::new(7000))
            .unwrap();

        assert_eq!(outcome.completion.status, CompletionStatus::Committed);
        assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
        assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
        assert_eq!(runtime.wal().replay_durable().len(), 3);
    }
}
