use andromeda_core::{Permission, ProcedureId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use std::sync::Arc;

use super::permission_evaluator::{PermissionDecision, PermissionEvaluator};
use crate::{CompletionStatus, InvocationContext, InvocationReject, InvocationRequest};

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

        if request.expected_contract_hash.is_zero() {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "expected ContractHash must not be zero before transaction creation"
                    .to_string(),
            });
        }

        if request.catalog_version.get() == 0 {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: "CatalogVersion must not be zero before transaction creation".to_string(),
            });
        }

        Ok(DecisionTrace {
            trace_id,
            decision: CriticalDecisionKind::ResourceGovernance,
            reason:
                "invocation id and declared contract identity admitted before transaction creation"
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
                return Ok(());
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
    fn test_admission_service_default() {
        let service = AdmissionService::default();
        assert!(service.permission_evaluator.is_none());
    }
}
