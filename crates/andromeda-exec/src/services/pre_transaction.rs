use andromeda_catalog::{ProcedureContractBinding, ProcedureContractRef};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{CompletionStatus, InvocationReject, InvocationRequest};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreTransactionValidationService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureBindingEvidence {
    Full(ProcedureContractBinding),
    LegacyRef(ProcedureContractRef),
    Missing,
}

impl From<ProcedureContractBinding> for ProcedureBindingEvidence {
    fn from(value: ProcedureContractBinding) -> Self {
        Self::Full(value)
    }
}

impl From<Option<ProcedureContractBinding>> for ProcedureBindingEvidence {
    fn from(value: Option<ProcedureContractBinding>) -> Self {
        value.map(Self::Full).unwrap_or(Self::Missing)
    }
}

impl From<ProcedureContractRef> for ProcedureBindingEvidence {
    fn from(value: ProcedureContractRef) -> Self {
        Self::LegacyRef(value)
    }
}

impl PreTransactionValidationService {
    pub fn validate_invocation_contract(
        request: &InvocationRequest,
        executable_binding: impl Into<ProcedureBindingEvidence>,
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        request.validate_admission(trace_id)?;

        let expected_binding = request.expected_binding.ok_or_else(|| InvocationReject {
            status: CompletionStatus::ContractRejected,
            reason: "ProcedureContractBinding missing before transaction creation".to_string(),
        })?;
        let executable_binding = require_full_executable_binding(executable_binding.into())?;

        if expected_binding.procedure_id != executable_binding.procedure_id {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "ProcedureContractBinding ProcedureId mismatch before transaction creation"
                    .to_string(),
            });
        }

        if expected_binding.contract_hash != executable_binding.contract_hash {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "ProcedureContractBinding ContractHash mismatch before transaction creation"
                        .to_string(),
            });
        }

        if expected_binding.catalog_version != executable_binding.catalog_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "ProcedureContractBinding CatalogVersion mismatch before transaction creation"
                        .to_string(),
            });
        }

        if expected_binding.stats_version != executable_binding.stats_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "ProcedureContractBinding StatsVersion mismatch before transaction creation"
                        .to_string(),
            });
        }

        if expected_binding.policy_version != executable_binding.policy_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "ProcedureContractBinding PolicyVersion mismatch before transaction creation"
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
            reason: "ProcedureId, ContractHash, CatalogVersion, StatsVersion, PolicyVersion, and structured parameter shapes accepted before transaction creation".to_string(),
        })
    }
}

fn require_full_executable_binding(
    evidence: ProcedureBindingEvidence,
) -> Result<ProcedureContractBinding, InvocationReject> {
    let binding = match evidence {
        ProcedureBindingEvidence::Full(binding) => binding,
        ProcedureBindingEvidence::LegacyRef(_) => {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "ProcedureContractBinding missing before transaction creation; ProcedureContractRef is insufficient".to_string(),
            });
        }
        ProcedureBindingEvidence::Missing => {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "ProcedureContractBinding missing before transaction creation".to_string(),
            });
        }
    };

    binding.validate().map_err(|error| InvocationReject {
        status: CompletionStatus::ContractRejected,
        reason: error.to_string(),
    })?;

    Ok(binding)
}
