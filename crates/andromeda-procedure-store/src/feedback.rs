use crate::{
    AuditCorrelationId, EvidenceDigest, InvocationIdentity, InvocationMetrics, InvocationStatus,
    ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult,
};
use andromeda_digest::Sha256;
use andromeda_procedure_contract::StatsVersion;
use andromeda_scenario_evidence::{ScenarioEvidenceError, ValidityWindow};
use andromeda_time::EngineTimestamp;
use andromeda_types::ProcedureId;

/// Domain tag absorbed at the start of every procedure-feedback digest.
const PROCEDURE_FEEDBACK_DOMAIN: &[u8] = b"andromeda.procedure_feedback.v0";

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

/// Bounded completion-status taxonomy mirroring terminal invocation outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompletionStatus {
    /// The invocation committed durably. Requires non-zero durable LSN evidence.
    Committed,
    /// The invocation rolled back cleanly. No durable LSN required.
    RolledBack,
    /// The invocation failed with a contract or runtime error.
    Failed,
    /// The invocation was aborted before completion.
    Aborted,
}

impl CompletionStatus {
    /// Stable tag byte folded into the feedback digest.
    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Committed => 0x01,
            Self::RolledBack => 0x02,
            Self::Failed => 0x03,
            Self::Aborted => 0x04,
        }
    }

    /// Total number of variants.
    pub const VARIANT_COUNT: usize = 4;
}

/// Bounded completion evidence drawn from an invocation completion trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompletionEvidence {
    pub status: CompletionStatus,
    pub completion_code: Option<u32>,
    pub row_count: Option<u64>,
    pub durable_lsn: Option<u64>,
}

impl CompletionEvidence {
    /// Validate the bounded structural invariants.
    pub fn validate(&self) -> Result<(), ProcedureFeedbackError> {
        if let Some(0) = self.durable_lsn {
            return Err(ProcedureFeedbackError::DurableLsnZero);
        }
        if let CompletionStatus::Committed = self.status {
            match self.durable_lsn {
                Some(lsn) if lsn != 0 => {},
                _ => return Err(ProcedureFeedbackError::CommittedRequiresDurableLsn),
            }
        }
        Ok(())
    }
}

/// V0 evidence-only feedback record for an observed invocation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcedureFeedback {
    feedback_id: FeedbackId,
    procedure_id: ProcedureId,
    plan_cache_key_digest: Option<[u8; 32]>,
    stats_version: StatsVersion,
    completion: CompletionEvidence,
    observed_window: ValidityWindow,
}

impl ProcedureFeedback {
    /// Construct a feedback record after running every bounded check.
    pub fn new(
        feedback_id: FeedbackId,
        procedure_id: ProcedureId,
        plan_cache_key_digest: Option<[u8; 32]>,
        stats_version: StatsVersion,
        completion: CompletionEvidence,
        observed_window: ValidityWindow,
    ) -> Result<Self, ProcedureFeedbackError> {
        if procedure_id.get() == 0 {
            return Err(ProcedureFeedbackError::ProcedureIdZero);
        }
        if stats_version.get() == 0 {
            return Err(ProcedureFeedbackError::StatsVersionZero);
        }
        if let Some(digest) = plan_cache_key_digest
            && digest == [0u8; 32]
        {
            return Err(ProcedureFeedbackError::PlanCacheKeyDigestZero);
        }
        completion.validate()?;
        Ok(Self {
            feedback_id,
            procedure_id,
            plan_cache_key_digest,
            stats_version,
            completion,
            observed_window,
        })
    }

    pub const fn feedback_id(&self) -> FeedbackId {
        self.feedback_id
    }

    pub const fn procedure_id(&self) -> ProcedureId {
        self.procedure_id
    }

    pub const fn plan_cache_key_digest(&self) -> Option<[u8; 32]> {
        self.plan_cache_key_digest
    }

    pub const fn stats_version(&self) -> StatsVersion {
        self.stats_version
    }

    pub const fn completion(&self) -> &CompletionEvidence {
        &self.completion
    }

    pub const fn observed_window(&self) -> ValidityWindow {
        self.observed_window
    }

    /// Procedure feedback is advisory evidence and is never authoritative.
    pub const fn is_authoritative(&self) -> bool {
        false
    }

    /// `true` when `now >= expires_at`.
    pub fn is_expired_at(&self, now: EngineTimestamp) -> bool {
        self.observed_window.is_expired_at(now)
    }

    /// Validate the record for consumption at `now`.
    pub fn validate_for_use_at(&self, now: EngineTimestamp) -> Result<(), ScenarioEvidenceError> {
        if self.observed_window.is_not_yet_valid_at(now) {
            return Err(ScenarioEvidenceError::NotYetValid);
        }
        if self.observed_window.is_expired_at(now) {
            return Err(ScenarioEvidenceError::Expired);
        }
        Ok(())
    }

    /// Compute a deterministic 32-byte digest over every field.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(PROCEDURE_FEEDBACK_DOMAIN);

        hasher.update(&[0xF0]);
        hasher.update(&self.feedback_id.get().to_le_bytes());

        hasher.update(&[0xF1]);
        hasher.update(&self.procedure_id.get().to_le_bytes());

        hasher.update(&[0xF2]);
        match self.plan_cache_key_digest {
            Some(d) => {
                hasher.update(&[0x01]);
                hasher.update(&d);
            },
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 32]);
            },
        }

        hasher.update(&[0xF3]);
        hasher.update(&self.stats_version.get().to_le_bytes());

        hasher.update(&[0xF4, self.completion.status.as_tag()]);
        hasher.update(&[0xF5]);
        match self.completion.completion_code {
            Some(c) => {
                hasher.update(&[0x01]);
                hasher.update(&c.to_le_bytes());
            },
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 4]);
            },
        }
        hasher.update(&[0xF6]);
        match self.completion.row_count {
            Some(r) => {
                hasher.update(&[0x01]);
                hasher.update(&r.to_le_bytes());
            },
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 8]);
            },
        }
        hasher.update(&[0xF7]);
        match self.completion.durable_lsn {
            Some(l) => {
                hasher.update(&[0x01]);
                hasher.update(&l.to_le_bytes());
            },
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 8]);
            },
        }

        hasher.update(&[0xF8]);
        hasher.update(
            &self
                .observed_window
                .issued_at()
                .as_unix_millis()
                .to_le_bytes(),
        );
        hasher.update(&[0xF9]);
        hasher.update(
            &self
                .observed_window
                .expires_at()
                .as_unix_millis()
                .to_le_bytes(),
        );
        hasher.update(&[0xFA, 0x00]);

        hasher.finalize()
    }
}

/// Closed set of reasons a [`ProcedureFeedback`] may be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureFeedbackError {
    /// `procedure_id` was zero.
    ProcedureIdZero,
    /// `stats_version` was zero.
    StatsVersionZero,
    /// `plan_cache_key_digest` was supplied but all-zero.
    PlanCacheKeyDigestZero,
    /// A `Some(0)` durable LSN was supplied.
    DurableLsnZero,
    /// `CompletionStatus::Committed` was supplied without a non-zero durable LSN.
    CommittedRequiresDurableLsn,
}

impl core::fmt::Display for ProcedureFeedbackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ProcedureIdZero => f.write_str("ProcedureFeedback.procedure_id must be non-zero"),
            Self::StatsVersionZero => {
                f.write_str("ProcedureFeedback.stats_version must be non-zero")
            },
            Self::PlanCacheKeyDigestZero => f.write_str(
                "ProcedureFeedback.plan_cache_key_digest, when present, must be non-zero",
            ),
            Self::DurableLsnZero => {
                f.write_str("CompletionEvidence.durable_lsn, when present, must be non-zero")
            },
            Self::CommittedRequiresDurableLsn => {
                f.write_str("CompletionStatus::Committed requires non-zero durable LSN evidence")
            },
        }
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
