use crate::{InvocationEvidenceMarker, InvocationIdentity, InvocationStatus};

pub trait InvocationEvidenceSink {
    type Error;

    fn record_status(
        &mut self,
        identity: InvocationIdentity,
        status: InvocationStatus,
    ) -> Result<(), Self::Error>;

    fn record_evidence(
        &mut self,
        identity: InvocationIdentity,
        evidence: InvocationEvidenceMarker,
    ) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;
    use crate::{EvidenceDigest, InvocationEvidenceKind, InvocationId, ProcedureId};

    #[derive(Default)]
    struct RecordingSink {
        statuses: Vec<(InvocationIdentity, InvocationStatus)>,
        evidence: Vec<(InvocationIdentity, InvocationEvidenceMarker)>,
    }

    impl InvocationEvidenceSink for RecordingSink {
        type Error = Infallible;

        fn record_status(
            &mut self,
            identity: InvocationIdentity,
            status: InvocationStatus,
        ) -> Result<(), Self::Error> {
            self.statuses.push((identity, status));
            Ok(())
        }

        fn record_evidence(
            &mut self,
            identity: InvocationIdentity,
            evidence: InvocationEvidenceMarker,
        ) -> Result<(), Self::Error> {
            self.evidence.push((identity, evidence));
            Ok(())
        }
    }

    #[test]
    fn sink_trait_records_status_and_evidence_without_runtime_storage() {
        let identity = InvocationIdentity::new(
            InvocationId::new(10).unwrap(),
            ProcedureId::new(20).unwrap(),
        );
        let marker = InvocationEvidenceMarker::new(
            InvocationEvidenceKind::RuntimeCounters,
            EvidenceDigest::new([8; EvidenceDigest::LEN]).unwrap(),
        );
        let mut sink = RecordingSink::default();

        sink.record_status(identity, InvocationStatus::Started)
            .unwrap();
        sink.record_evidence(identity, marker).unwrap();

        assert_eq!(sink.statuses, vec![(identity, InvocationStatus::Started)]);
        assert_eq!(sink.evidence, vec![(identity, marker)]);
    }
}
