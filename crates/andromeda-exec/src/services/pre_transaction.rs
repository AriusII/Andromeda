use andromeda_catalog::ProcedureContractRef;
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{CompletionStatus, InvocationReject, InvocationRequest};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreTransactionValidationService;

impl PreTransactionValidationService {
    pub fn validate_invocation_contract(
        request: &InvocationRequest,
        executable_contract: ProcedureContractRef,
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        executable_contract
            .validate()
            .map_err(|error| InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: error.to_string(),
            })?;

        if request.procedure.procedure_id != executable_contract.procedure_id {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "ProcedureId mismatch before transaction creation".to_string(),
            });
        }

        if request.procedure.contract_hash != executable_contract.contract_hash {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "Executable ContractHash mismatch before transaction creation".to_string(),
            });
        }

        if request.procedure.catalog_version != executable_contract.catalog_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "Executable CatalogVersion mismatch before transaction creation"
                    .to_string(),
            });
        }

        if request.procedure.contract_hash != request.expected_contract_hash {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "ContractHash mismatch before transaction creation".to_string(),
            });
        }

        if request.procedure.catalog_version != request.catalog_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "CatalogVersion mismatch before transaction creation".to_string(),
            });
        }

        for parameter in &request.structured_parameters {
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
