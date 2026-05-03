use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::{CompletionStatus, InvocationReject};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationContext {
    pub trace_id: TraceId,
    pub granted_permissions: Vec<String>,
}

impl InvocationContext {
    pub fn new(trace_id: TraceId, granted_permissions: Vec<String>) -> Self {
        Self {
            trace_id,
            granted_permissions,
        }
    }

    pub fn grants(&self, permission: &str) -> bool {
        self.granted_permissions
            .iter()
            .any(|granted| granted == permission)
    }

    pub fn authorize(
        &self,
        required_permissions: &[String],
    ) -> Result<DecisionTrace, InvocationReject> {
        for permission in required_permissions {
            if !self.grants(permission) {
                return Err(InvocationReject {
                    status: CompletionStatus::PermissionDenied,
                    reason: format!("missing required permission: {permission}"),
                });
            }
        }

        Ok(DecisionTrace {
            trace_id: self.trace_id,
            decision: CriticalDecisionKind::SecurityAuthorization,
            reason: format!(
                "{} required permissions accepted before transaction creation",
                required_permissions.len()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_accepts_all_required_permissions() {
        let context = InvocationContext::new(
            TraceId::new(10),
            vec!["Inventory.ReserveStock.Execute".to_string()],
        );

        let trace = context
            .authorize(&["Inventory.ReserveStock.Execute".to_string()])
            .unwrap();

        assert_eq!(trace.decision, CriticalDecisionKind::SecurityAuthorization);
        assert!(trace.has_explanation());
    }

    #[test]
    fn admission_rejects_missing_permissions() {
        let context = InvocationContext::new(TraceId::new(10), Vec::new());

        let reject = context
            .authorize(&["Inventory.ReserveStock.Execute".to_string()])
            .unwrap_err();

        assert_eq!(reject.status, CompletionStatus::PermissionDenied);
        assert!(reject.reason.contains("Inventory.ReserveStock.Execute"));
    }
}
