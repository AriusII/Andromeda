use crate::{
    AuditCorrelationId, EvidenceDigest, InvocationIdentity, InvocationMetrics, InvocationStatus,
    ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeedbackId(u64);

impl FeedbackId {
    pub fn new(value: u64) -> ProcedureStorePrimitiveResult<Self> {
        if value == 0 {
            return Err(ProcedureStorePrimitiveError::ZeroFeedbackId);
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvocationFeedback {
    feedback_id: FeedbackId,
    identity: InvocationIdentity,
    status: InvocationStatus,
    metrics: InvocationMetrics,
    evidence_digest: EvidenceDigest,
    audit_correlation_id: Option<AuditCorrelationId>,
}

impl InvocationFeedback {
    pub fn new(
        feedback_id: FeedbackId,
        identity: InvocationIdentity,
        status: InvocationStatus,
        metrics: InvocationMetrics,
        evidence_digest: EvidenceDigest,
    ) -> ProcedureStorePrimitiveResult<Self> {
        if !status.is_terminal() {
            return Err(ProcedureStorePrimitiveError::NonTerminalFeedbackStatus);
        }

        Ok(Self {
            feedback_id,
            identity,
            status,
            metrics,
            evidence_digest,
            audit_correlation_id: None,
        })
    }

    pub const fn with_audit_correlation_id(mut self, correlation_id: AuditCorrelationId) -> Self {
        self.audit_correlation_id = Some(correlation_id);
        self
    }

    pub const fn feedback_id(self) -> FeedbackId {
        self.feedback_id
    }

    pub const fn identity(self) -> InvocationIdentity {
        self.identity
    }

    pub const fn status(self) -> InvocationStatus {
        self.status
    }

    pub const fn metrics(self) -> InvocationMetrics {
        self.metrics
    }

    pub const fn evidence_digest(self) -> EvidenceDigest {
        self.evidence_digest
    }

    pub const fn audit_correlation_id(self) -> Option<AuditCorrelationId> {
        self.audit_correlation_id
    }

    pub const fn is_observed_feedback(self) -> bool {
        true
    }

    pub const fn is_authoritative(self) -> bool {
        false
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
    use crate::{InvocationId, ProcedureId};

    #[test]
    fn feedback_id_rejects_zero() {
        assert_eq!(
            FeedbackId::new(0).unwrap_err(),
            ProcedureStorePrimitiveError::ZeroFeedbackId
        );
        assert_eq!(FeedbackId::new(7).unwrap().get(), 7);
    }

    #[test]
    fn invocation_feedback_requires_terminal_status_and_remains_advisory() {
        let identity =
            InvocationIdentity::new(InvocationId::new(1).unwrap(), ProcedureId::new(2).unwrap());
        let metrics = InvocationMetrics::new(19, 4, 1, 32, 0);
        let digest = EvidenceDigest::new([9; EvidenceDigest::LEN]).unwrap();

        assert_eq!(
            InvocationFeedback::new(
                FeedbackId::new(1).unwrap(),
                identity,
                InvocationStatus::Started,
                metrics,
                digest,
            )
            .unwrap_err(),
            ProcedureStorePrimitiveError::NonTerminalFeedbackStatus
        );

        let feedback = InvocationFeedback::new(
            FeedbackId::new(1).unwrap(),
            identity,
            InvocationStatus::Committed,
            metrics,
            digest,
        )
        .unwrap()
        .with_audit_correlation_id(AuditCorrelationId::new(3).unwrap());

        assert_eq!(feedback.identity(), identity);
        assert_eq!(feedback.status(), InvocationStatus::Committed);
        assert_eq!(feedback.metrics(), metrics);
        assert_eq!(feedback.evidence_digest(), digest);
        assert_eq!(feedback.audit_correlation_id().unwrap().get(), 3);
        assert!(feedback.is_observed_feedback());
        assert!(!feedback.is_authoritative());
        assert!(!feedback.can_select_plan_alone());
        assert!(!feedback.is_durable_truth());
    }
}
