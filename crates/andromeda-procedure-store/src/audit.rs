use crate::{
    EvidenceDigest, InvocationIdentity, ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuditCorrelationId(u128);

impl AuditCorrelationId {
    pub fn new(value: u128) -> ProcedureStorePrimitiveResult<Self> {
        if value == 0 {
            return Err(ProcedureStorePrimitiveError::ZeroAuditCorrelationId);
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> u128 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuditCorrelation {
    identity: InvocationIdentity,
    correlation_id: AuditCorrelationId,
    audit_digest: EvidenceDigest,
    decision_trace_digest: Option<EvidenceDigest>,
}

impl AuditCorrelation {
    pub const fn new(
        identity: InvocationIdentity,
        correlation_id: AuditCorrelationId,
        audit_digest: EvidenceDigest,
    ) -> Self {
        Self {
            identity,
            correlation_id,
            audit_digest,
            decision_trace_digest: None,
        }
    }

    pub const fn with_decision_trace_digest(mut self, digest: EvidenceDigest) -> Self {
        self.decision_trace_digest = Some(digest);
        self
    }

    pub const fn identity(self) -> InvocationIdentity {
        self.identity
    }

    pub const fn correlation_id(self) -> AuditCorrelationId {
        self.correlation_id
    }

    pub const fn audit_digest(self) -> EvidenceDigest {
        self.audit_digest
    }

    pub const fn decision_trace_digest(self) -> Option<EvidenceDigest> {
        self.decision_trace_digest
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
    fn audit_correlation_rejects_zero_id() {
        assert_eq!(
            AuditCorrelationId::new(0).unwrap_err(),
            ProcedureStorePrimitiveError::ZeroAuditCorrelationId
        );
        assert_eq!(AuditCorrelationId::new(9).unwrap().get(), 9);
    }

    #[test]
    fn audit_correlation_binds_invocation_to_audit_digest_only() {
        let identity =
            InvocationIdentity::new(InvocationId::new(1).unwrap(), ProcedureId::new(2).unwrap());
        let audit_digest = EvidenceDigest::new([3; EvidenceDigest::LEN]).unwrap();
        let decision_digest = EvidenceDigest::new([4; EvidenceDigest::LEN]).unwrap();
        let correlation =
            AuditCorrelation::new(identity, AuditCorrelationId::new(5).unwrap(), audit_digest)
                .with_decision_trace_digest(decision_digest);

        assert_eq!(correlation.identity(), identity);
        assert_eq!(correlation.correlation_id().get(), 5);
        assert_eq!(correlation.audit_digest(), audit_digest);
        assert_eq!(correlation.decision_trace_digest(), Some(decision_digest));
        assert!(!correlation.is_durable_truth());
    }
}
