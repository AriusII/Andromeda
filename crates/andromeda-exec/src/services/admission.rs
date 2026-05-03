use andromeda_observe::{CriticalDecisionKind, DecisionTrace};

use crate::{CompletionStatus, InvocationContext, InvocationReject};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdmissionService;

impl AdmissionService {
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
