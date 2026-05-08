use crate::{
    AuditCorrelationId, InvocationEvidenceMarker, InvocationIdentity, InvocationMetrics,
    InvocationStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvocationHistoryRecord {
    identity: InvocationIdentity,
    status: InvocationStatus,
    evidence: InvocationEvidenceMarker,
    metrics: Option<InvocationMetrics>,
    audit_correlation_id: Option<AuditCorrelationId>,
}

impl InvocationHistoryRecord {
    pub const fn new(
        identity: InvocationIdentity,
        status: InvocationStatus,
        evidence: InvocationEvidenceMarker,
    ) -> Self {
        Self {
            identity,
            status,
            evidence,
            metrics: None,
            audit_correlation_id: None,
        }
    }

    pub const fn with_metrics(mut self, metrics: InvocationMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub const fn with_audit_correlation_id(mut self, correlation_id: AuditCorrelationId) -> Self {
        self.audit_correlation_id = Some(correlation_id);
        self
    }

    pub const fn identity(self) -> InvocationIdentity {
        self.identity
    }

    pub const fn status(self) -> InvocationStatus {
        self.status
    }

    pub const fn evidence(self) -> InvocationEvidenceMarker {
        self.evidence
    }

    pub const fn metrics(self) -> Option<InvocationMetrics> {
        self.metrics
    }

    pub const fn audit_correlation_id(self) -> Option<AuditCorrelationId> {
        self.audit_correlation_id
    }

    pub const fn is_terminal(self) -> bool {
        self.status.is_terminal()
    }

    pub const fn can_select_plan_alone(self) -> bool {
        false
    }

    pub const fn is_durable_truth(self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvidenceDigest, InvocationEvidenceKind, InvocationId, ProcedureId};

    #[test]
    fn history_record_carries_status_evidence_metrics_and_audit_correlation() {
        let identity =
            InvocationIdentity::new(InvocationId::new(1).unwrap(), ProcedureId::new(2).unwrap());
        let evidence = InvocationEvidenceMarker::new(
            InvocationEvidenceKind::Metrics,
            EvidenceDigest::new([6; EvidenceDigest::LEN]).unwrap(),
        );
        let metrics = InvocationMetrics::new(21, 3, 1, 64, 0);
        let correlation_id = AuditCorrelationId::new(8).unwrap();
        let record = InvocationHistoryRecord::new(identity, InvocationStatus::Committed, evidence)
            .with_metrics(metrics)
            .with_audit_correlation_id(correlation_id);

        assert_eq!(record.identity(), identity);
        assert_eq!(record.status(), InvocationStatus::Committed);
        assert_eq!(record.evidence(), evidence);
        assert_eq!(record.metrics(), Some(metrics));
        assert_eq!(record.audit_correlation_id(), Some(correlation_id));
        assert!(record.is_terminal());
        assert!(!record.can_select_plan_alone());
        assert!(!record.is_durable_truth());
    }
}
