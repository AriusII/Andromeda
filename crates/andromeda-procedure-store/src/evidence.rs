use crate::{ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvidenceDigest([u8; Self::LEN]);

impl EvidenceDigest {
    pub const LEN: usize = 32;

    pub fn new(bytes: [u8; Self::LEN]) -> ProcedureStorePrimitiveResult<Self> {
        let digest = Self(bytes);
        if digest.is_zero() {
            return Err(ProcedureStorePrimitiveError::ZeroEvidenceDigest);
        }

        Ok(digest)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvocationEvidenceKind {
    Admission,
    Authorization,
    DecisionTrace,
    RuntimeCounters,
    Feedback,
    Metrics,
    RegressionSignal,
    AuditCorrelation,
    Completion,
}

impl InvocationEvidenceKind {
    pub const VARIANT_COUNT: usize = 9;

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Admission => 0x01,
            Self::Authorization => 0x02,
            Self::DecisionTrace => 0x03,
            Self::RuntimeCounters => 0x04,
            Self::Feedback => 0x05,
            Self::Metrics => 0x06,
            Self::RegressionSignal => 0x07,
            Self::AuditCorrelation => 0x08,
            Self::Completion => 0x09,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvocationEvidenceMarker {
    pub kind: InvocationEvidenceKind,
    pub digest: EvidenceDigest,
}

impl InvocationEvidenceMarker {
    pub const fn new(kind: InvocationEvidenceKind, digest: EvidenceDigest) -> Self {
        Self { kind, digest }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_digest_rejects_zero_digest() {
        assert_eq!(
            EvidenceDigest::new([0; EvidenceDigest::LEN]).unwrap_err(),
            ProcedureStorePrimitiveError::ZeroEvidenceDigest
        );
        assert_eq!(
            EvidenceDigest::new([3; EvidenceDigest::LEN])
                .unwrap()
                .as_bytes(),
            [3; EvidenceDigest::LEN]
        );
    }

    #[test]
    fn evidence_marker_carries_kind_and_digest_only() {
        let digest = EvidenceDigest::new([5; EvidenceDigest::LEN]).unwrap();
        let marker = InvocationEvidenceMarker::new(InvocationEvidenceKind::DecisionTrace, digest);

        assert_eq!(marker.kind, InvocationEvidenceKind::DecisionTrace);
        assert_eq!(marker.digest.as_bytes(), [5; EvidenceDigest::LEN]);
    }

    #[test]
    fn evidence_kind_tags_cover_procedure_store_evidence() {
        assert_eq!(InvocationEvidenceKind::VARIANT_COUNT, 9);
        assert_eq!(InvocationEvidenceKind::Feedback.as_tag(), 0x05);
        assert_eq!(InvocationEvidenceKind::Metrics.as_tag(), 0x06);
        assert_eq!(InvocationEvidenceKind::RegressionSignal.as_tag(), 0x07);
        assert_eq!(InvocationEvidenceKind::AuditCorrelation.as_tag(), 0x08);
    }
}
