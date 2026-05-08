use super::policy::{AuditEmissionPolicy, durable_family_to_audit_family};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditReplayBehavior, DurableAuditRetentionBoundary,
    DurableAuditSinkReport, TraceId,
};

pub type AuditEmissionKind = andromeda_audit::AuditEmissionKind;
pub type AuditEmissionOutcome = andromeda_audit::AuditEmissionOutcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSinkAvailability {
    durable_sink_available: bool,
    reason: Option<String>,
    durability: Option<AuditSinkDurabilityEvidence>,
}

impl AuditSinkAvailability {
    pub fn available() -> Self {
        Self {
            durable_sink_available: true,
            reason: None,
            durability: None,
        }
    }

    pub fn durable(report: DurableAuditSinkReport) -> AndromedaResult<Self> {
        Ok(Self::from_durability(
            AuditSinkDurabilityEvidence::from_report(report)?,
        ))
    }

    pub fn durable_for_policy(
        policy: AuditEmissionPolicy,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<Self> {
        match policy.expected_event_family() {
            Some(expected_family) => Self::durable_for_visible_decision(expected_family, report),
            None => Self::durable(report),
        }
    }

    pub fn durable_for_visible_decision(
        expected_family: DurableAuditEventFamily,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<Self> {
        let durability = AuditSinkDurabilityEvidence::from_report(report)?;
        durability.validate_visible_decision(expected_family)?;
        Ok(Self::from_durability(durability))
    }

    pub fn durable_for_security_decision(report: DurableAuditSinkReport) -> AndromedaResult<Self> {
        Self::durable_for_visible_decision(DurableAuditEventFamily::SecurityDecision, report)
    }

    pub fn durable_for_admin_decision(report: DurableAuditSinkReport) -> AndromedaResult<Self> {
        Self::durable_for_visible_decision(DurableAuditEventFamily::AdminDecision, report)
    }

    pub fn durable_for_catalog_decision(report: DurableAuditSinkReport) -> AndromedaResult<Self> {
        Self::durable_for_visible_decision(DurableAuditEventFamily::CatalogDecision, report)
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            durable_sink_available: false,
            reason: Some(andromeda_audit::redact_audit_reason(reason)),
            durability: None,
        }
    }

    pub const fn is_available(&self) -> bool {
        self.durable_sink_available
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub const fn durability(&self) -> Option<AuditSinkDurabilityEvidence> {
        self.durability
    }

    pub(crate) fn to_audit(&self) -> AndromedaResult<andromeda_audit::AuditSinkAvailability> {
        if let Some(durability) = self.durability {
            return andromeda_audit::AuditSinkAvailability::durable(durability.to_audit_report());
        }
        if self.durable_sink_available {
            Ok(andromeda_audit::AuditSinkAvailability::available())
        } else {
            Ok(andromeda_audit::AuditSinkAvailability::unavailable(
                self.reason
                    .clone()
                    .unwrap_or_else(|| "durable audit sink unavailable".to_string()),
            ))
        }
    }

    fn from_durability(durability: AuditSinkDurabilityEvidence) -> Self {
        Self {
            durable_sink_available: true,
            reason: None,
            durability: Some(durability),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditSinkDurabilityEvidence {
    pub trace_id: TraceId,
    pub family: DurableAuditEventFamily,
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub retention: DurableAuditRetentionBoundary,
}

impl AuditSinkDurabilityEvidence {
    pub fn from_report(report: DurableAuditSinkReport) -> AndromedaResult<Self> {
        report.validate()?;
        Ok(Self {
            trace_id: report.identity.trace_id,
            family: report.identity.family,
            record_lsn: report.evidence.record_lsn,
            durable_lsn: report.evidence.durable_lsn,
            checksum: report.evidence.checksum,
            replay_behavior: report.replay_behavior,
            retention: report.retention,
        })
    }

    pub const fn proves_durable(self) -> bool {
        self.record_lsn != 0 && self.durable_lsn >= self.record_lsn && self.checksum != 0
    }

    pub fn validate_trace_id(self, expected_trace_id: TraceId) -> AndromedaResult<()> {
        if self.trace_id != expected_trace_id {
            return Err(audit_emission_error(format!(
                "durable audit WAL evidence trace id mismatch: expected {}, got {}",
                expected_trace_id.get(),
                self.trace_id.get()
            )));
        }
        Ok(())
    }

    pub fn validate_visible_decision(
        self,
        expected_family: DurableAuditEventFamily,
    ) -> AndromedaResult<()> {
        self.to_audit()
            .validate_visible_decision(durable_family_to_audit_family(expected_family))
    }

    fn to_audit(self) -> andromeda_audit::AuditSinkDurabilityEvidence {
        andromeda_audit::AuditSinkDurabilityEvidence {
            trace_id: self.trace_id,
            family: durable_family_to_audit_family(self.family),
            record_lsn: self.record_lsn,
            durable_lsn: self.durable_lsn,
            checksum: self.checksum,
            replay_behavior: replay_behavior_to_audit(self.replay_behavior),
            retention: retention_boundary_to_audit(self.retention),
        }
    }

    fn to_audit_report(self) -> andromeda_audit::AuditSinkDurabilityReport {
        andromeda_audit::AuditSinkDurabilityReport {
            trace_id: self.trace_id,
            family: durable_family_to_audit_family(self.family),
            sequence_number: self.record_lsn,
            record_lsn: self.record_lsn,
            durable_lsn: self.durable_lsn,
            checksum: self.checksum,
            replay_behavior: replay_behavior_to_audit(self.replay_behavior),
            retention: retention_boundary_to_audit(self.retention),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEmissionEvidence {
    pub trace_id: TraceId,
    pub kind: AuditEmissionKind,
    pub outcome: AuditEmissionOutcome,
    pub reason: String,
    pub sink: AuditSinkAvailability,
}

pub type PermissionAuditEvidence = AuditEmissionEvidence;

impl AuditEmissionEvidence {
    pub fn new(
        policy: AuditEmissionPolicy,
        trace_id: TraceId,
        kind: AuditEmissionKind,
        outcome: AuditEmissionOutcome,
        reason: impl Into<String>,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<Self> {
        let audit_evidence = andromeda_audit::AuditEmissionEvidence::new(
            policy.into_audit(),
            trace_id,
            kind,
            outcome,
            reason,
            sink.to_audit()?,
        )?;
        Ok(Self::from_audit_with_sink(audit_evidence, sink))
    }

    pub fn contract_rejected(
        policy: AuditEmissionPolicy,
        trace_id: TraceId,
        reason: impl Into<String>,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<Self> {
        Self::new(
            policy,
            trace_id,
            AuditEmissionKind::ContractRejection,
            AuditEmissionOutcome::Rejected,
            reason,
            sink,
        )
    }

    pub fn completion_emitted(
        policy: AuditEmissionPolicy,
        trace_id: TraceId,
        reason: impl Into<String>,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<Self> {
        Self::new(
            policy,
            trace_id,
            AuditEmissionKind::Completion,
            AuditEmissionOutcome::Emitted,
            reason,
            sink,
        )
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.to_audit()?.validate()
    }

    fn to_audit(&self) -> AndromedaResult<andromeda_audit::AuditEmissionEvidence> {
        Ok(andromeda_audit::AuditEmissionEvidence {
            trace_id: self.trace_id,
            kind: self.kind,
            outcome: self.outcome,
            reason: self.reason.clone(),
            sink: self.sink.to_audit()?,
        })
    }

    fn from_audit_with_sink(
        audit_evidence: andromeda_audit::AuditEmissionEvidence,
        sink: AuditSinkAvailability,
    ) -> Self {
        Self {
            trace_id: audit_evidence.trace_id,
            kind: audit_evidence.kind,
            outcome: audit_evidence.outcome,
            reason: audit_evidence.reason,
            sink,
        }
    }
}

pub(crate) fn audit_emission_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

const fn replay_behavior_to_audit(
    replay: DurableAuditReplayBehavior,
) -> andromeda_audit::AuditEmissionReplayBehavior {
    match replay {
        DurableAuditReplayBehavior::ForensicOnly => {
            andromeda_audit::AuditEmissionReplayBehavior::ForensicOnly
        }
        DurableAuditReplayBehavior::RebuildDecisionIndex => {
            andromeda_audit::AuditEmissionReplayBehavior::RebuildDecisionIndex
        }
        DurableAuditReplayBehavior::CorruptionBoundary => {
            andromeda_audit::AuditEmissionReplayBehavior::CorruptionBoundary
        }
    }
}

const fn retention_boundary_to_audit(
    retention: DurableAuditRetentionBoundary,
) -> andromeda_audit::AuditEmissionRetentionBoundary {
    match retention {
        DurableAuditRetentionBoundary::WalSegment => {
            andromeda_audit::AuditEmissionRetentionBoundary::WalSegment
        }
        DurableAuditRetentionBoundary::CatalogVersion => {
            andromeda_audit::AuditEmissionRetentionBoundary::CatalogVersion
        }
        DurableAuditRetentionBoundary::SecurityPolicy => {
            andromeda_audit::AuditEmissionRetentionBoundary::SecurityPolicy
        }
        DurableAuditRetentionBoundary::ForensicHold => {
            andromeda_audit::AuditEmissionRetentionBoundary::ForensicHold
        }
    }
}
