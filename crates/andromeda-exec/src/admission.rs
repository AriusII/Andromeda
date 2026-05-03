use andromeda_observe::TraceId;

use crate::{InvocationReject, services::AdmissionService};

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
    ) -> Result<andromeda_observe::DecisionTrace, InvocationReject> {
        AdmissionService::authorize(self, required_permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observe::CriticalDecisionKind;

    use crate::CompletionStatus;

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
