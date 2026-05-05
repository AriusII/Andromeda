use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_core::{Permission, ProcedureId, AndromedaErrorKind, AndromedaError};
use std::sync::Arc;

use crate::{CompletionStatus, InvocationContext, InvocationReject, InvocationRequest};
use super::permission_evaluator::{PermissionEvaluator, PermissionDecision};

/// Admission control service: contract validation + IAM authorization.
///
/// This service validates invocation requests and enforces RBAC permissions
/// before transaction creation and execution.
///
/// ## Authorization Flow
///
/// 1. Validate invocation request (InvocationId, contract hash, etc.)
/// 2. Extract certificate fingerprint from QUIC connection context
/// 3. Evaluate principal's permissions using PermissionEvaluator
/// 4. Emit audit event (PermissionCheckInitiated)
/// 5. Return decision (allow/deny) with audit trail
/// 6. If allowed, proceed to transaction creation
/// 7. If denied, emit PermissionCheckResult (denied) and reject request
#[derive(Debug, Clone)]
pub struct AdmissionService {
    /// Permission evaluator for authorization checks.
    pub permission_evaluator: Option<Arc<dyn PermissionEvaluator>>,
}

impl Default for AdmissionService {
    fn default() -> Self {
        Self {
            permission_evaluator: None,
        }
    }
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

    /// Evaluate IAM permission for a principal (Wave 19 integration point).
    ///
    /// This method is called by the admission control pipeline to enforce RBAC
    /// permissions before transaction creation. It:
    /// 1. Resolves the principal from certificate fingerprint
    /// 2. Checks if the principal has the required permission
    /// 3. Returns decision for audit logging
    ///
    /// # Arguments
    ///
    /// * `cert_fingerprint` - X.509 certificate fingerprint from QUIC connection
    /// * `required_permission` - Permission required for the action
    /// * `procedure_id` - Procedure being invoked (for audit trail)
    /// * `trace_id` - Trace ID for audit event correlation
    ///
    /// # Returns
    ///
    /// `Ok(())` if permission is granted
    /// `Err(InvocationReject)` if permission is denied
    pub fn evaluate_permission(
        &self,
        cert_fingerprint: &str,
        required_permission: &Permission,
        _procedure_id: ProcedureId,
        _trace_id: TraceId,
    ) -> Result<(), InvocationReject> {
        // If no evaluator is configured, allow all access (development mode)
        let evaluator = match &self.permission_evaluator {
            Some(e) => e,
            None => {
                // No permission evaluator configured; default allow
                return Ok(());
            }
        };

        // TODO(Wave 19): Implement audit event emission (PermissionCheckInitiated)
        // This will be integrated with andromeda-observe::admission_audit

        // Evaluate permission
        let decision = evaluator.evaluate_permission(cert_fingerprint, required_permission);

        match decision {
            PermissionDecision::Allowed { principal_id, .. } => {
                // TODO(Wave 19): Emit PermissionCheckResult(Allowed)
                Ok(())
            }
            PermissionDecision::Denied {
                principal_id,
                reason,
                ..
            } => {
                // TODO(Wave 19): Emit PermissionCheckResult(Denied)
                Err(InvocationReject {
                    status: CompletionStatus::PermissionDenied,
                    reason: format!(
                        "access denied: {} [principal: {:?}, reason: {}]",
                        required_permission,
                        principal_id.map(|id| id.get()),
                        reason
                    ),
                })
            }
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
