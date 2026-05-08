use andromeda_observe::{DecisionTrace, TraceId};

use crate::{AdmissionService, InvocationReject};

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
        AdmissionService::authorize(self, required_permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompletionStatus;
    use andromeda_core::{RequestId, SessionId};
    use andromeda_observe::{
        CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, TraceEvent,
    };

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

    #[test]
    fn admission_denial_can_be_emitted_without_transaction_evidence() {
        let permission = "Inventory.ReserveStock.Execute";
        let context = InvocationContext::new(TraceId::new(10), Vec::new());

        let reject = context.authorize(&[permission.to_string()]).unwrap_err();
        let trace = reject
            .authorization_denial_trace(context.trace_id, permission)
            .expect("permission denial maps to authorization denial evidence");

        let envelope = EventEnvelope::new(
            EventId::new(11),
            EventCorrelation {
                request_id: Some(RequestId::new(12)),
                session_id: Some(SessionId::new(13)),
                contract_hash: None,
                catalog_version: None,
                catalog_object_id: None,
                transaction_id: None,
                durable_lsn: None,
                protocol: None,
            },
            TraceEvent::AuthorizationDenied(trace),
        )
        .expect("authorization denial evidence is request/session correlated");

        assert!(envelope.correlation.has_no_transaction_evidence());
    }
}
