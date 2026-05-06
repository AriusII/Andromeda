//! Permission decision audit events for IAM admission.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, Permission, PrincipalId,
};
use andromeda_observe::{
    CriticalDecisionKind, DecisionTrace, DurableAuditEventFamily, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, DurableAuditSinkReport, TraceId,
};
use std::fmt;

const UNKNOWN_PRINCIPAL_ID: PrincipalId = PrincipalId::new(0);
const REDACTED_AUDIT_REASON: &str = "[redacted sensitive audit evidence]";
const UNSPECIFIED_AUDIT_REASON: &str = "unspecified audit evidence";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEmissionPolicy {
    fail_closed_when_sink_unavailable: bool,
    require_durable_wal_evidence: bool,
    expected_event_family: Option<DurableAuditEventFamily>,
}

impl AuditEmissionPolicy {
    pub const fn fail_closed() -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: false,
            expected_event_family: None,
        }
    }

    pub const fn fail_closed_with_durable_wal() -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: true,
            expected_event_family: None,
        }
    }

    pub const fn fail_closed_for_visible_decision(
        expected_event_family: DurableAuditEventFamily,
    ) -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: true,
            expected_event_family: Some(expected_event_family),
        }
    }

    pub const fn allow_unavailable_sink_for_tests() -> Self {
        Self {
            fail_closed_when_sink_unavailable: false,
            require_durable_wal_evidence: false,
            expected_event_family: None,
        }
    }

    pub const fn requires_available_sink(self) -> bool {
        self.fail_closed_when_sink_unavailable
    }

    pub const fn requires_durable_wal_evidence(self) -> bool {
        self.require_durable_wal_evidence
    }

    pub const fn expected_event_family(self) -> Option<DurableAuditEventFamily> {
        self.expected_event_family
    }
}

impl Default for AuditEmissionPolicy {
    fn default() -> Self {
        Self::fail_closed()
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditSinkDurabilityEvidence {
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

    pub fn validate_visible_decision(
        self,
        expected_family: DurableAuditEventFamily,
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
        if policy.requires_available_sink() && !evidence.sink.is_available() {
            return Err(audit_emission_error(format!(
                "durable audit sink unavailable for {} {}: {}",
                evidence.kind.as_str(),
                evidence.outcome.as_str(),
                evidence
                    .sink
                    .reason()
                    .unwrap_or("durable audit sink unavailable")
            )));
        }
        if policy.requires_durable_wal_evidence() {
            let Some(durability) = evidence.sink.durability() else {
                return Err(audit_emission_error(format!(
                    "durable audit WAL evidence required before visible {} {} decision",
                    evidence.kind.as_str(),
                    evidence.outcome.as_str()
                )));
            };
            if !durability.proves_durable() {
                return Err(audit_emission_error(
                    "durable audit WAL evidence did not prove append and flush",
                ));
            }
            if let Some(expected) = policy.expected_event_family() {
                durability.validate_visible_decision(expected)?;
            }
        }
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionAuditEvent {
    pub trace_id: TraceId,
    pub principal_id: PrincipalId,
    pub required_permission: Permission,
    pub decision: PermissionDecisionAudit,
    pub timestamp: std::time::SystemTime,
}

impl PermissionAuditEvent {
    pub fn allowed(
        trace_id: TraceId,
        principal_id: PrincipalId,
        required_permission: Permission,
    ) -> Self {
        Self {
            trace_id,
            principal_id,
            required_permission,
            decision: PermissionDecisionAudit::Allowed,
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn denied(
        trace_id: TraceId,
        principal_id: PrincipalId,
        required_permission: Permission,
        reason: DenialAuditReason,
    ) -> Self {
        Self {
            trace_id,
            principal_id,
            required_permission,
            decision: PermissionDecisionAudit::Denied(reason),
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn denied_unknown_principal(trace_id: TraceId, required_permission: Permission) -> Self {
        Self {
            trace_id,
            principal_id: UNKNOWN_PRINCIPAL_ID,
            required_permission,
            decision: PermissionDecisionAudit::DeniedUnknownPrincipal,
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn to_decision_trace(&self) -> DecisionTrace {
        let reason = match &self.decision {
            PermissionDecisionAudit::Allowed => {
                format!(
                    "permission allowed: principal {} granted permission {}",
                    self.principal_id, self.required_permission
                )
            }
            PermissionDecisionAudit::Denied(reason) => {
                format!(
                    "permission denied: principal {} - {}",
                    self.principal_id,
                    reason.explanation()
                )
            }
            PermissionDecisionAudit::DeniedUnknownPrincipal => {
                format!(
                    "permission denied: unknown principal - permission required: {}",
                    self.required_permission
                )
            }
        };

        DecisionTrace {
            trace_id: self.trace_id,
            decision: CriticalDecisionKind::SecurityAuthorization,
            reason,
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.decision, PermissionDecisionAudit::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        matches!(
            self.decision,
            PermissionDecisionAudit::Denied(_) | PermissionDecisionAudit::DeniedUnknownPrincipal
        )
    }

    pub fn audit_evidence(
        &self,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<PermissionAuditEvidence> {
        let outcome = if self.is_allowed() {
            AuditEmissionOutcome::Allowed
        } else {
            AuditEmissionOutcome::Denied
        };
        AuditEmissionEvidence::new(
            policy,
            self.trace_id,
            AuditEmissionKind::PermissionDecision,
            outcome,
            self.to_decision_trace().reason,
            sink,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecisionAudit {
    Allowed,
    Denied(DenialAuditReason),
    DeniedUnknownPrincipal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialAuditReason {
    NoPermissionsGranted,
    PermissionNotGranted,
    ProcedureIdMismatch,
    SuperAdminOperationNotAudited,
    WildcardDeniedByPolicy,
    SessionExpired,
    CertificateRevoked,
    InternalError,
}

impl DenialAuditReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoPermissionsGranted => "no_permissions_granted",
            Self::PermissionNotGranted => "permission_not_granted",
            Self::ProcedureIdMismatch => "procedure_id_mismatch",
            Self::SuperAdminOperationNotAudited => "superadmin_operation_not_audited",
            Self::WildcardDeniedByPolicy => "wildcard_denied_by_policy",
            Self::SessionExpired => "session_expired",
            Self::CertificateRevoked => "certificate_revoked",
            Self::InternalError => "internal_error",
        }
    }

    pub fn explanation(self) -> String {
        match self {
            Self::NoPermissionsGranted => "principal has no permissions granted".to_string(),
            Self::PermissionNotGranted => {
                "required permission is not in principal's permission set".to_string()
            }
            Self::ProcedureIdMismatch => {
                "requested procedure ID does not match granted procedure".to_string()
            }
            Self::SuperAdminOperationNotAudited => {
                "super-admin operation attempted without explicit authorization audit record"
                    .to_string()
            }
            Self::WildcardDeniedByPolicy => {
                "wildcard permission was explicitly denied by policy".to_string()
            }
            Self::SessionExpired => "principal's session has expired".to_string(),
            Self::CertificateRevoked => "certificate revocation check failed".to_string(),
            Self::InternalError => "internal error during permission evaluation".to_string(),
        }
    }
}

impl fmt::Display for DenialAuditReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub trait PermissionAuditEmitter: Send + Sync {
    fn emit_permission_decision(&self, event: PermissionAuditEvent) -> AndromedaResult<()>;

    fn emit_permission_decision_with_policy(
        &self,
        event: PermissionAuditEvent,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<PermissionAuditEvidence> {
        let evidence = event.audit_evidence(policy, sink)?;
        self.emit_permission_decision(event)?;
        Ok(evidence)
    }
}

pub struct NoOpPermissionAuditEmitter;

impl PermissionAuditEmitter for NoOpPermissionAuditEmitter {
    fn emit_permission_decision(&self, _event: PermissionAuditEvent) -> AndromedaResult<()> {
        Ok(())
    }
}

pub fn redact_audit_reason(reason: impl Into<String>) -> String {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return UNSPECIFIED_AUDIT_REASON.to_string();
    }
    if audit_text_contains_sensitive_marker(&reason) {
        return REDACTED_AUDIT_REASON.to_string();
    }
    reason
}

pub fn audit_text_contains_sensitive_marker(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    [
        "authorization:",
        "bearer ",
        "credential=",
        "password=",
        "private key",
        "private_key",
        "secret=",
        "token=",
        "x-api-key",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn audit_emission_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

const fn retention_boundary_is_visible_decision_compatible(
    family: DurableAuditEventFamily,
    boundary: DurableAuditRetentionBoundary,
) -> bool {
    match family {
        DurableAuditEventFamily::SecurityDecision
        | DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision => matches!(
            boundary,
            DurableAuditRetentionBoundary::SecurityPolicy
                | DurableAuditRetentionBoundary::ForensicHold
        ),
        DurableAuditEventFamily::CatalogDecision => matches!(
            boundary,
            DurableAuditRetentionBoundary::CatalogVersion
                | DurableAuditRetentionBoundary::ForensicHold
        ),
        DurableAuditEventFamily::BackupDecision | DurableAuditEventFamily::RestoreDecision => {
            matches!(
                boundary,
                DurableAuditRetentionBoundary::WalSegment
                    | DurableAuditRetentionBoundary::ForensicHold
            )
        }
        DurableAuditEventFamily::ForensicDecision => {
            matches!(boundary, DurableAuditRetentionBoundary::ForensicHold)
        }
        DurableAuditEventFamily::AdmissionDecision
        | DurableAuditEventFamily::RecoveryDecision
        | DurableAuditEventFamily::GenericAudit => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_allowed() {
        let event = PermissionAuditEvent::allowed(
            TraceId::new(1),
            PrincipalId::new(100),
            Permission::ExecuteProcedure(andromeda_core::ProcedureId::new(1)),
        );

        assert!(event.is_allowed());
        assert!(!event.is_denied());
    }

    #[test]
    fn test_audit_event_denied() {
        let event = PermissionAuditEvent::denied(
            TraceId::new(1),
            PrincipalId::new(100),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        assert!(!event.is_allowed());
        assert!(event.is_denied());
    }

    #[test]
    fn test_audit_event_denied_unknown_principal() {
        let event = PermissionAuditEvent::denied_unknown_principal(
            TraceId::new(1),
            Permission::AdminCatalogPublish,
        );

        assert!(!event.is_allowed());
        assert!(event.is_denied());
    }

    #[test]
    fn test_denial_reason_explanation() {
        assert!(
            !DenialAuditReason::NoPermissionsGranted
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::PermissionNotGranted
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::ProcedureIdMismatch
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::SuperAdminOperationNotAudited
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::WildcardDeniedByPolicy
                .explanation()
                .is_empty()
        );
    }

    #[test]
    fn test_decision_trace_conversion() {
        let event = PermissionAuditEvent::allowed(
            TraceId::new(100),
            PrincipalId::new(42),
            Permission::ReadContractMetadata,
        );

        let trace = event.to_decision_trace();
        assert_eq!(trace.trace_id, TraceId::new(100));
        assert_eq!(trace.decision, CriticalDecisionKind::SecurityAuthorization);
        assert!(trace.reason.contains("permission allowed"));
    }

    #[test]
    fn test_noop_emitter_accepts_all_events() {
        let emitter = NoOpPermissionAuditEmitter;
        let event = PermissionAuditEvent::denied(
            TraceId::new(1),
            PrincipalId::new(100),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        let result = emitter.emit_permission_decision(event);
        assert!(result.is_ok());
    }

    #[test]
    fn audit_evidence_redacts_sensitive_reason_markers() -> AndromedaResult<()> {
        let evidence = AuditEmissionEvidence::contract_rejected(
            AuditEmissionPolicy::fail_closed(),
            TraceId::new(77),
            "contract mismatch carried token=abc123 in a payload diagnostic",
            AuditSinkAvailability::available(),
        )?;

        assert_eq!(evidence.reason, REDACTED_AUDIT_REASON);
        assert!(!audit_text_contains_sensitive_marker(&evidence.reason));
        Ok(())
    }

    #[test]
    fn audit_evidence_fails_closed_when_durable_sink_is_unavailable() {
        let error = AuditEmissionEvidence::contract_rejected(
            AuditEmissionPolicy::fail_closed(),
            TraceId::new(78),
            "contract rejected before transaction",
            AuditSinkAvailability::unavailable("failed to open audit sink: secret=raw"),
        )
        .expect_err("strict audit policy must fail closed when durable sink is unavailable");

        assert_eq!(error.kind(), AndromedaErrorKind::Security);
        assert!(error.message().contains("durable audit sink unavailable"));
        assert!(!audit_text_contains_sensitive_marker(error.message()));
    }

    #[test]
    fn permission_emitter_consumes_policy_and_returns_evidence() -> AndromedaResult<()> {
        let emitter = NoOpPermissionAuditEmitter;
        let event = PermissionAuditEvent::denied(
            TraceId::new(79),
            PrincipalId::new(100),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        let evidence = emitter.emit_permission_decision_with_policy(
            event,
            AuditEmissionPolicy::fail_closed(),
            AuditSinkAvailability::available(),
        )?;

        assert_eq!(evidence.kind, AuditEmissionKind::PermissionDecision);
        assert_eq!(evidence.outcome, AuditEmissionOutcome::Denied);
        assert!(evidence.sink.is_available());
        Ok(())
    }
}
