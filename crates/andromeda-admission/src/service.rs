use andromeda_observability::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_principal::Permission;
use andromeda_types::ProcedureId;
use std::sync::Arc;

use crate::{
    InvocationContext, InvocationReject, InvocationRequest, PermissionDecision, PermissionEvaluator,
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
            return Err(InvocationReject::contract_rejected(
                "InvocationId must not be zero before transaction creation",
            ));
        }

        request
            .procedure
            .validate()
            .map_err(|error| InvocationReject::contract_rejected(error.to_string()))?;

        let expected_binding = request.expected_binding.ok_or_else(|| {
            InvocationReject::contract_rejected(
                "ProcedureContractBinding missing before transaction creation",
            )
        })?;

        expected_binding
            .validate()
            .map_err(|error| InvocationReject::contract_rejected(error.to_string()))?;

        if expected_binding.as_legacy_ref() != request.procedure {
            return Err(InvocationReject::contract_rejected(
                "ProcedureContractBinding does not match declared Procedure contract before transaction creation",
            ));
        }

        if request.expected_contract_hash.is_zero() {
            return Err(InvocationReject::contract_rejected(
                "expected ContractHash must not be zero before transaction creation",
            ));
        }

        if request.expected_contract_hash != expected_binding.contract_hash {
            return Err(InvocationReject::contract_rejected(
                "expected ContractHash mismatch against ProcedureContractBinding before transaction creation",
            ));
        }

        if request.catalog_version.get() == 0 {
            return Err(InvocationReject::contract_rejected(
                "CatalogVersion must not be zero before transaction creation",
            ));
        }

        if request.catalog_version != expected_binding.catalog_version {
            return Err(InvocationReject::contract_rejected(
                "CatalogVersion mismatch against ProcedureContractBinding before transaction creation",
            ));
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
                return Err(InvocationReject::permission_denied(format!(
                    "missing required permission: {permission}"
                )));
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
                return Err(InvocationReject::permission_denied(
                    "permission evaluator unavailable before Procedure admission",
                ));
            },
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
            } => Err(InvocationReject::permission_denied(format!(
                "access denied: {} [principal: {:?}, reason: {}]",
                required_permission,
                principal_id.map(|id| id.get()),
                reason
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompletionStatus;

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
