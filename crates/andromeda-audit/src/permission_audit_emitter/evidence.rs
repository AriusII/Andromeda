use super::policy::AuditEmissionPolicy;
use super::redaction::{audit_text_contains_sensitive_marker, redact_audit_reason};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::TraceId;

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

    pub fn durable(report: AuditSinkDurabilityReport) -> AndromedaResult<Self> {
        Ok(Self::from_durability(
            AuditSinkDurabilityEvidence::from_report(report)?,
        ))
    }

    pub fn durable_for_policy(
        policy: AuditEmissionPolicy,
        report: AuditSinkDurabilityReport,
    ) -> AndromedaResult<Self> {
        match policy.expected_event_family() {
            Some(expected_family) => Self::durable_for_visible_decision(expected_family, report),
            None => Self::durable(report),
        }
    }

    pub fn durable_for_visible_decision(
        expected_family: AuditEmissionEventFamily,
        report: AuditSinkDurabilityReport,
    ) -> AndromedaResult<Self> {
        let durability = AuditSinkDurabilityEvidence::from_report(report)?;
        durability.validate_visible_decision(expected_family)?;
        Ok(Self::from_durability(durability))
    }

    pub fn durable_for_security_decision(
        report: AuditSinkDurabilityReport,
    ) -> AndromedaResult<Self> {
        Self::durable_for_visible_decision(AuditEmissionEventFamily::SecurityDecision, report)
    }

    pub fn durable_for_admin_decision(report: AuditSinkDurabilityReport) -> AndromedaResult<Self> {
        Self::durable_for_visible_decision(AuditEmissionEventFamily::AdminDecision, report)
    }

    pub fn durable_for_catalog_decision(
        report: AuditSinkDurabilityReport,
    ) -> AndromedaResult<Self> {
        Self::durable_for_visible_decision(AuditEmissionEventFamily::CatalogDecision, report)
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            durable_sink_available: false,
            reason: Some(redact_audit_reason(reason)),
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

    fn from_durability(durability: AuditSinkDurabilityEvidence) -> Self {
        Self {
            durable_sink_available: true,
            reason: None,
            durability: Some(durability),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditEmissionEventFamily {
    SecurityDecision,
    AdminDecision,
    AdmissionDecision,
    CatalogDecision,
    HadrDecision,
    BackupDecision,
    RestoreDecision,
    ForensicDecision,
    RecoveryDecision,
    GenericAudit,
}

impl AuditEmissionEventFamily {
    pub const fn requires_wal_before_visible_decision(self) -> bool {
        matches!(
            self,
            Self::SecurityDecision
                | Self::AdminDecision
                | Self::CatalogDecision
                | Self::HadrDecision
                | Self::BackupDecision
                | Self::RestoreDecision
                | Self::ForensicDecision
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditEmissionReplayBehavior {
    ForensicOnly,
    RebuildDecisionIndex,
    CorruptionBoundary,
}

impl AuditEmissionReplayBehavior {
    pub const fn is_visible_decision_evidence(self) -> bool {
        matches!(self, Self::ForensicOnly | Self::RebuildDecisionIndex)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditEmissionRetentionBoundary {
    WalSegment,
    CatalogVersion,
    SecurityPolicy,
    ForensicHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditSinkDurabilityReport {
    pub trace_id: TraceId,
    pub family: AuditEmissionEventFamily,
    pub sequence_number: u64,
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
    pub replay_behavior: AuditEmissionReplayBehavior,
    pub retention: AuditEmissionRetentionBoundary,
}

impl AuditSinkDurabilityReport {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.trace_id.is_zero() {
            return Err(audit_emission_error(
                "durable audit sink report requires non-zero trace_id",
            ));
        }
        if self.sequence_number == 0 {
            return Err(audit_emission_error(
                "durable audit sink report requires non-zero sequence_number",
            ));
        }
        if !self.proves_durable() {
            return Err(audit_emission_error(
                "durable audit sink report requires non-zero WAL LSN/checksum evidence",
            ));
        }
        Ok(())
    }

    pub const fn proves_durable(self) -> bool {
        self.record_lsn != 0 && self.durable_lsn >= self.record_lsn && self.checksum != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditSinkDurabilityEvidence {
    pub trace_id: TraceId,
    pub family: AuditEmissionEventFamily,
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
    pub replay_behavior: AuditEmissionReplayBehavior,
    pub retention: AuditEmissionRetentionBoundary,
}

impl AuditSinkDurabilityEvidence {
    pub fn from_report(report: AuditSinkDurabilityReport) -> AndromedaResult<Self> {
        report.validate()?;
        Ok(Self {
            trace_id: report.trace_id,
            family: report.family,
            record_lsn: report.record_lsn,
            durable_lsn: report.durable_lsn,
            checksum: report.checksum,
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
        expected_family: AuditEmissionEventFamily,
    ) -> AndromedaResult<()> {
        if !expected_family.requires_wal_before_visible_decision() {
            return Err(audit_emission_error(format!(
                "durable audit WAL evidence expected family is not a visible decision family: {:?}",
                expected_family
            )));
        }
        if self.family != expected_family {
            return Err(audit_emission_error(format!(
                "durable audit WAL evidence family mismatch: expected {:?}, got {:?}",
                expected_family, self.family
            )));
        }
        if !self.proves_durable() {
            return Err(audit_emission_error(
                "durable audit WAL evidence did not prove append and flush",
            ));
        }
        if !self.replay_behavior.is_visible_decision_evidence() {
            return Err(audit_emission_error(
                "durable audit WAL evidence replay behavior is not visible-decision evidence",
            ));
        }
        if !retention_boundary_is_visible_decision_compatible(expected_family, self.retention) {
            return Err(audit_emission_error(
                "durable audit WAL evidence retention boundary is not compatible with visible decision family",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEmissionKind {
    PermissionDecision,
    ContractRejection,
    Completion,
}

impl AuditEmissionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermissionDecision => "permission_decision",
            Self::ContractRejection => "contract_rejection",
            Self::Completion => "completion",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEmissionOutcome {
    Allowed,
    Denied,
    Rejected,
    Emitted,
}

impl AuditEmissionOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Rejected => "rejected",
            Self::Emitted => "emitted",
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
        let evidence = Self {
            trace_id,
            kind,
            outcome,
            reason: redact_audit_reason(reason),
            sink,
        };
        evidence.validate()?;
        evidence.validate_policy(policy)?;
        Ok(evidence)
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
        if self.trace_id.is_zero() {
            return Err(audit_emission_error(
                "audit emission evidence requires nonzero trace id",
            ));
        }
        if self.reason.trim().is_empty() {
            return Err(audit_emission_error(
                "audit emission evidence requires reason evidence",
            ));
        }
        if audit_text_contains_sensitive_marker(&self.reason) {
            return Err(audit_emission_error(
                "audit emission evidence reason must be redacted before emission",
            ));
        }
        if self
            .sink
            .reason()
            .is_some_and(audit_text_contains_sensitive_marker)
        {
            return Err(audit_emission_error(
                "audit sink availability reason must be redacted before emission",
            ));
        }
        Ok(())
    }

    fn validate_policy(&self, policy: AuditEmissionPolicy) -> AndromedaResult<()> {
        self.validate_sink_availability(policy)?;
        self.validate_durable_wal_evidence(policy)
    }

    fn validate_sink_availability(&self, policy: AuditEmissionPolicy) -> AndromedaResult<()> {
        if policy.requires_available_sink() && !self.sink.is_available() {
            return Err(audit_emission_error(format!(
                "durable audit sink unavailable for {} {}: {}",
                self.kind.as_str(),
                self.outcome.as_str(),
                self.sink
                    .reason()
                    .unwrap_or("durable audit sink unavailable")
            )));
        }
        Ok(())
    }

    fn validate_durable_wal_evidence(&self, policy: AuditEmissionPolicy) -> AndromedaResult<()> {
        if !policy.requires_durable_wal_evidence() {
            return Ok(());
        }

        let Some(durability) = self.sink.durability() else {
            return Err(audit_emission_error(format!(
                "durable audit WAL evidence required before visible {} {} decision",
                self.kind.as_str(),
                self.outcome.as_str()
            )));
        };

        durability.validate_trace_id(self.trace_id)?;
        if !durability.proves_durable() {
            return Err(audit_emission_error(
                "durable audit WAL evidence did not prove append and flush",
            ));
        }
        if let Some(expected) = policy.expected_event_family() {
            durability.validate_visible_decision(expected)?;
        }
        Ok(())
    }
}

pub fn audit_emission_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

const fn retention_boundary_is_visible_decision_compatible(
    family: AuditEmissionEventFamily,
    boundary: AuditEmissionRetentionBoundary,
) -> bool {
    match family {
        AuditEmissionEventFamily::SecurityDecision
        | AuditEmissionEventFamily::AdminDecision
        | AuditEmissionEventFamily::HadrDecision => matches!(
            boundary,
            AuditEmissionRetentionBoundary::SecurityPolicy
                | AuditEmissionRetentionBoundary::ForensicHold
        ),
        AuditEmissionEventFamily::CatalogDecision => matches!(
            boundary,
            AuditEmissionRetentionBoundary::CatalogVersion
                | AuditEmissionRetentionBoundary::ForensicHold
        ),
        AuditEmissionEventFamily::BackupDecision | AuditEmissionEventFamily::RestoreDecision => {
            matches!(
                boundary,
                AuditEmissionRetentionBoundary::WalSegment
                    | AuditEmissionRetentionBoundary::ForensicHold
            )
        },
        AuditEmissionEventFamily::ForensicDecision => {
            matches!(boundary, AuditEmissionRetentionBoundary::ForensicHold)
        },
        AuditEmissionEventFamily::AdmissionDecision
        | AuditEmissionEventFamily::RecoveryDecision
        | AuditEmissionEventFamily::GenericAudit => false,
    }
}
