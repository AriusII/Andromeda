use andromeda_core::{Permission, ProcedureId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use std::sync::Arc;

use crate::{
    CompletionStatus, InvocationContext, InvocationReject, InvocationRequest, PermissionDecision,
    PermissionEvaluator,
};

#[derive(Clone, Default)]
pub struct AdmissionService {
    pub permission_evaluator: Option<Arc<dyn PermissionEvaluator>>,
}

impl AdmissionService {
    pub fn new(permission_evaluator: Arc<dyn PermissionEvaluator>) -> Self {
        Self {
            permission_evaluator: Some(permission_evaluator),
        }
    }

    pub fn validate_invocation_request(
        request: &InvocationRequest,
        trace_id: TraceId,
    ) -> Result<DecisionTrace, InvocationReject> {
        if request.invocation_id.get() == 0 {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "InvocationId must not be zero before transaction creation".to_string(),
            });
        }

        request
            .procedure
            .validate()
            .map_err(|error| InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: error.to_string(),
            })?;

        let expected_binding = request.expected_binding.ok_or_else(|| InvocationReject {
            status: CompletionStatus::ContractRejected,
            reason: "ProcedureContractBinding missing before transaction creation".to_string(),
        })?;

        expected_binding
            .validate()
            .map_err(|error| InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: error.to_string(),
            })?;

        if expected_binding.as_legacy_ref() != request.procedure {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "ProcedureContractBinding does not match declared Procedure contract before transaction creation"
                        .to_string(),
            });
        }

        if request.expected_contract_hash.is_zero() {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "expected ContractHash must not be zero before transaction creation"
                    .to_string(),
            });
        }

        if request.expected_contract_hash != expected_binding.contract_hash {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "expected ContractHash mismatch against ProcedureContractBinding before transaction creation"
                        .to_string(),
            });
        }

        if request.catalog_version.get() == 0 {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "CatalogVersion must not be zero before transaction creation".to_string(),
            });
        }

        if request.catalog_version != expected_binding.catalog_version {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason:
                    "CatalogVersion mismatch against ProcedureContractBinding before transaction creation"
                        .to_string(),
            });
        }

        Ok(DecisionTrace {
            trace_id,
            decision: CriticalDecisionKind::ResourceGovernance,
            reason:
                "invocation id and full ProcedureContractBinding admitted before transaction creation"
                    .to_string(),
        })
    }

    pub fn authorize(
        context: &InvocationContext,
        required_permissions: &[String],
    ) -> Result<DecisionTrace, InvocationReject> {
        for permission in required_permissions {
            if !context.grants(permission) {
                return Err(InvocationReject {
                    status: CompletionStatus::PermissionDenied,
                    reason: format!("missing required permission: {permission}"),
                });
            }
        }

        Ok(DecisionTrace {
            trace_id: context.trace_id,
            decision: CriticalDecisionKind::SecurityAuthorization,
            reason: format!(
                "{} required permissions accepted before transaction creation",
                required_permissions.len()
            ),
        })
    }

    pub fn evaluate_permission(
        &self,
        cert_fingerprint: &str,
        required_permission: &Permission,
        _procedure_id: ProcedureId,
        _trace_id: TraceId,
    ) -> Result<(), InvocationReject> {
        let evaluator = match &self.permission_evaluator {
            Some(e) => e,
            None => {
                return Err(InvocationReject {
                    status: CompletionStatus::PermissionDenied,
                    reason: "permission evaluator unavailable before Procedure admission"
                        .to_string(),
                });
            }
        };

        let decision = evaluator.evaluate_permission(cert_fingerprint, required_permission);

        match decision {
            PermissionDecision::Allowed {
                principal_id: _, ..
            } => Ok(()),
            PermissionDecision::Denied {
                principal_id,
                reason,
                ..
            } => Err(InvocationReject {
                status: CompletionStatus::PermissionDenied,
                reason: format!(
                    "access denied: {} [principal: {:?}, reason: {}]",
                    required_permission,
                    principal_id.map(|id| id.get()),
                    reason
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_service_default_fails_closed_for_permission_evaluation() {
        let service = AdmissionService::default();

        let reject = service
            .evaluate_permission(
                "missing-evaluator",
                &Permission::AdminShutdown,
                ProcedureId::new(1),
                TraceId::new(1),
            )
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::PermissionDenied);
        assert!(reject.reason.contains("permission evaluator unavailable"));
    }
}
