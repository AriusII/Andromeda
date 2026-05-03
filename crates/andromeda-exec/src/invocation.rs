use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaResult, CatalogVersion, ContractHash, InvocationId};
use andromeda_observe::{DecisionTrace, TraceId};
use andromeda_proto::StructuredObjectHeader;

use crate::{CompletionStatus, InvocationCompletion, services::PreTransactionValidationService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationRequest {
    pub invocation_id: InvocationId,
    pub procedure: ProcedureContractRef,
    pub expected_contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub structured_parameters: Vec<StructuredObjectHeader>,
}

impl InvocationRequest {
    pub fn validate_before_transaction(
        &self,
        executable_contract: ProcedureContractRef,
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        PreTransactionValidationService::validate_invocation_contract(
            self,
            executable_contract,
            trace_id,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationReject {
    pub status: CompletionStatus,
    pub reason: String,
}

pub trait ProcedureInvoker {
    fn invoke(&self, request: InvocationRequest) -> AndromedaResult<InvocationCompletion>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{CatalogVersion, ProcedureId};

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
    fn invocation_rejects_contract_mismatch_before_transaction_creation() {
        let reject = request(ContractHash::test_vector(8))
            .validate_before_transaction(
                request(ContractHash::test_vector(7)).procedure,
                TraceId::new(1),
            )
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
    }

    #[test]
    fn invocation_accepts_contract_before_transaction_creation() {
        let trace = request(ContractHash::test_vector(7))
            .validate_before_transaction(
                request(ContractHash::test_vector(7)).procedure,
                TraceId::new(1),
            )
            .unwrap();

        assert!(trace.has_explanation());
    }

    #[test]
    fn invocation_rejects_procedure_id_mismatch_before_transaction_creation() {
        let mut executable = request(ContractHash::test_vector(7)).procedure;
        executable.procedure_id = ProcedureId::new(99);

        let reject = request(ContractHash::test_vector(7))
            .validate_before_transaction(executable, TraceId::new(1))
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("ProcedureId"));
    }
}
