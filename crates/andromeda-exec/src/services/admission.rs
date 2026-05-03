use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{CompletionStatus, InvocationContext, InvocationReject, InvocationRequest};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdmissionService;

impl AdmissionService {
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
}
