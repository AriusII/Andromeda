use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaResult, CatalogVersion, ContractHash, InvocationId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_proto::StructuredObjectHeader;

use crate::{CompletionStatus, InvocationCompletion};

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
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        if self.procedure.contract_hash != self.expected_contract_hash {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "ContractHash mismatch before transaction creation".to_string(),
            });
        }

        if self.procedure.catalog_version != self.catalog_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "CatalogVersion mismatch before transaction creation".to_string(),
            });
        }

        for parameter in &self.structured_parameters {
            parameter.validate().map_err(|error| InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: error.to_string(),
            })?;
        }

        Ok(DecisionTrace {
            trace_id,
            decision: CriticalDecisionKind::ContractValidation,
            reason: "contract hash, catalog version, and structured parameter shapes accepted"
                .to_string(),
        })
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
            .validate_before_transaction(TraceId::new(1))
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
    }

    #[test]
    fn invocation_accepts_contract_before_transaction_creation() {
        let trace = request(ContractHash::test_vector(7))
            .validate_before_transaction(TraceId::new(1))
            .unwrap();

        assert!(trace.has_explanation());
    }
}
