use andromeda_error::AndromedaResult;
use andromeda_observability::{EventId, TraceId};
use andromeda_types::{RequestId, SessionId};

use crate::helpers::{audit_error, contains_sensitive_marker};
use crate::identity::SecurityPolicyVersionEvidence;
use crate::scope::{Permission, SurfaceScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditEventFamily {
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

impl DurableAuditEventFamily {
    pub const ALL: [Self; 10] = [
        Self::SecurityDecision,
        Self::AdminDecision,
        Self::AdmissionDecision,
        Self::CatalogDecision,
        Self::HadrDecision,
        Self::BackupDecision,
        Self::RestoreDecision,
        Self::ForensicDecision,
        Self::RecoveryDecision,
        Self::GenericAudit,
    ];

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
pub enum DurableAuditFailureKind {
    ValidationRejected,
    WalAppendRejected,
    WalFlushRejected,
    CorruptionDetected,
    PermissionDenied,
    RetentionRejected,
}

impl DurableAuditFailureKind {
    /// Durable-audit failures are fail-closed by doctrine.
    pub const fn requires_fail_closed(self) -> bool {
        match self {
            Self::ValidationRejected
            | Self::WalAppendRejected
            | Self::WalFlushRejected
            | Self::CorruptionDetected
            | Self::PermissionDenied
            | Self::RetentionRejected => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DurableAuditRecordIdentity {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub family: DurableAuditEventFamily,
    pub sequence_number: u64,
}

impl DurableAuditRecordIdentity {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.event_id.is_zero() {
            return Err(audit_error(
                "durable audit record identity requires non-zero event_id",
            ));
        }
        if self.trace_id.is_zero() {
            return Err(audit_error(
                "durable audit record identity requires non-zero trace_id",
            ));
        }
        if self.sequence_number == 0 {
            return Err(audit_error(
                "durable audit record identity requires non-zero sequence_number",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditWalEvidence {
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
}

impl DurableAuditWalEvidence {
    /// Evidence is accepted only after append + flush.
    pub const fn proves_durable(self) -> bool {
        self.record_lsn != 0 && self.durable_lsn >= self.record_lsn && self.checksum != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditReplayBehavior {
    /// Replay is forensic-only and must not re-authorize or re-execute.
    ForensicOnly,
    RebuildDecisionIndex,
    CorruptionBoundary,
}

impl DurableAuditReplayBehavior {
    pub const fn is_visible_decision_evidence(self) -> bool {
        matches!(self, Self::ForensicOnly | Self::RebuildDecisionIndex)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditRetentionBoundary {
    WalSegment,
    CatalogVersion,
    SecurityPolicy,
    ForensicHold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditPrincipalBinding {
    pub principal_id: String,
    pub certificate_fingerprint: Option<String>,
    pub surface: Option<SurfaceScope>,
    pub permission: Option<Permission>,
    pub policy_version: Option<SecurityPolicyVersionEvidence>,
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
}

impl DurableAuditPrincipalBinding {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.principal_id.trim().is_empty() {
            return Err(audit_error(
                "durable audit principal binding requires non-empty principal_id",
            ));
        }

        let certificate_is_empty = self
            .certificate_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| fingerprint.trim().is_empty());
        if certificate_is_empty {
            return Err(audit_error(
                "durable audit principal binding certificate fingerprint must not be empty when present",
            ));
        }

        if contains_sensitive_marker(&self.principal_id)
            || self
                .certificate_fingerprint
                .as_ref()
                .is_some_and(|fingerprint| contains_sensitive_marker(fingerprint))
            || self
                .policy_version
                .as_ref()
                .is_some_and(SecurityPolicyVersionEvidence::contains_sensitive_evidence)
        {
            return Err(audit_error(
                "durable audit principal binding must not contain secret evidence",
            ));
        }

        if self
            .policy_version
            .as_ref()
            .is_some_and(|evidence| !evidence.has_version_evidence())
        {
            return Err(audit_error(
                "durable audit principal binding policy version evidence must be non-zero and canonical",
            ));
        }

        if let (Some(surface), Some(permission)) = (self.surface, self.permission)
            && !surface.permits_permission(permission)
        {
            return Err(audit_error(
                "durable audit principal binding surface must permit permission evidence",
            ));
        }

        if self.request_id.is_some() != self.session_id.is_some() {
            return Err(audit_error(
                "durable audit principal binding request/session ids must be present together",
            ));
        }

        if self
            .request_id
            .is_some_and(|request_id| request_id.get() == 0)
            || self
                .session_id
                .is_some_and(|session_id| session_id.get() == 0)
        {
            return Err(audit_error(
                "durable audit principal binding request/session ids must be non-zero when present",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableAuditPolicyEvidenceRequirement {
    NotRequired,
    PermissionedCriticalDecision,
}

impl DurableAuditPolicyEvidenceRequirement {
    pub const fn requires_policy_evidence(self) -> bool {
        matches!(self, Self::PermissionedCriticalDecision)
    }
}

pub fn classify_policy_evidence_requirement(
    family: DurableAuditEventFamily,
    binding: &DurableAuditPrincipalBinding,
) -> DurableAuditPolicyEvidenceRequirement {
    match family {
        DurableAuditEventFamily::SecurityDecision
        | DurableAuditEventFamily::AdminDecision
        | DurableAuditEventFamily::HadrDecision
        | DurableAuditEventFamily::BackupDecision
        | DurableAuditEventFamily::RestoreDecision
        | DurableAuditEventFamily::ForensicDecision => {
            DurableAuditPolicyEvidenceRequirement::PermissionedCriticalDecision
        },
        DurableAuditEventFamily::CatalogDecision if binding.permission.is_some() => {
            DurableAuditPolicyEvidenceRequirement::PermissionedCriticalDecision
        },
        DurableAuditEventFamily::RecoveryDecision if binding.permission.is_some() => {
            DurableAuditPolicyEvidenceRequirement::PermissionedCriticalDecision
        },
        DurableAuditEventFamily::AdmissionDecision
        | DurableAuditEventFamily::CatalogDecision
        | DurableAuditEventFamily::RecoveryDecision
        | DurableAuditEventFamily::GenericAudit => {
            DurableAuditPolicyEvidenceRequirement::NotRequired
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditSinkReport {
    pub identity: DurableAuditRecordIdentity,
    pub evidence: DurableAuditWalEvidence,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub retention: DurableAuditRetentionBoundary,
}

impl DurableAuditSinkReport {
    pub fn validate(self) -> AndromedaResult<()> {
        self.identity.validate()?;
        if !self.evidence.proves_durable() {
            return Err(audit_error(
                "durable audit sink report requires non-zero WAL LSN/checksum evidence",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayLsnRange {
    pub start_lsn: u64,
    pub end_lsn: u64,
}

impl DurableAuditReplayLsnRange {
    pub const fn new(start_lsn: u64, end_lsn: u64) -> Self {
        Self { start_lsn, end_lsn }
    }

    pub const fn contains(self, lsn: u64) -> bool {
        self.start_lsn <= lsn && lsn <= self.end_lsn
    }

    pub const fn is_valid(self) -> bool {
        self.start_lsn != 0 && self.end_lsn != 0 && self.start_lsn <= self.end_lsn
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DurableAuditReplayQuery {
    pub family: Option<DurableAuditEventFamily>,
    pub trace_id: Option<TraceId>,
    pub principal_id: Option<String>,
    pub lsn_range: Option<DurableAuditReplayLsnRange>,
}

impl DurableAuditReplayQuery {
    pub fn all() -> Self {
        Self::default()
    }

    pub const fn has_filter(&self) -> bool {
        self.family.is_some()
            || self.trace_id.is_some()
            || self.principal_id.is_some()
            || self.lsn_range.is_some()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if let Some(trace_id) = self.trace_id
            && trace_id.is_zero()
        {
            return Err(audit_error(
                "durable audit replay trace_id filter must be non-zero when present",
            ));
        }
        if let Some(principal_id) = &self.principal_id {
            if principal_id.trim().is_empty() {
                return Err(audit_error(
                    "durable audit replay principal filter must be non-empty when present",
                ));
            }
            if contains_sensitive_marker(principal_id) {
                return Err(audit_error(
                    "durable audit replay principal filter must not contain secret evidence",
                ));
            }
        }
        if let Some(range) = self.lsn_range
            && !range.is_valid()
        {
            return Err(audit_error(
                "durable audit replay LSN range must be non-zero and start_lsn <= end_lsn",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayWindow {
    pub limit: usize,
    pub offset: usize,
}

impl DurableAuditReplayWindow {
    pub const ALL: Self = Self {
        limit: usize::MAX,
        offset: 0,
    };

    pub const fn new(limit: usize, offset: usize) -> Self {
        Self { limit, offset }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.limit == 0 {
            return Err(audit_error(
                "durable audit replay window limit must be non-zero",
            ));
        }
        Ok(())
    }
}

impl Default for DurableAuditReplayWindow {
    fn default() -> Self {
        Self::ALL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayEvidence {
    pub records_scanned: usize,
    pub records_matched: usize,
    pub records_returned: usize,
    pub filter_applied: bool,
    pub limit: usize,
    pub offset: usize,
    pub truncated: bool,
    pub first_returned_lsn: Option<u64>,
    pub last_returned_lsn: Option<u64>,
    pub chain_anchor_present: bool,
    pub first_scanned_lsn: Option<u64>,
    pub last_scanned_lsn: Option<u64>,
    pub tail_chain_checksum: u64,
}

impl DurableAuditReplayEvidence {
    pub const fn empty(query: &DurableAuditReplayQuery, window: DurableAuditReplayWindow) -> Self {
        Self {
            records_scanned: 0,
            records_matched: 0,
            records_returned: 0,
            filter_applied: query.has_filter(),
            limit: window.limit,
            offset: window.offset,
            truncated: false,
            first_returned_lsn: None,
            last_returned_lsn: None,
            chain_anchor_present: false,
            first_scanned_lsn: None,
            last_scanned_lsn: None,
            tail_chain_checksum: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditReplayRecord {
    pub report: DurableAuditSinkReport,
    pub principal_binding: DurableAuditPrincipalBinding,
    pub event_kind: String,
}

impl DurableAuditReplayRecord {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.report.validate()?;
        self.principal_binding.validate()?;
        if self.event_kind.trim().is_empty() {
            return Err(audit_error(
                "durable audit replay record requires event kind evidence",
            ));
        }
        if contains_sensitive_marker(&self.event_kind) {
            return Err(audit_error(
                "durable audit replay event kind must not contain secret evidence",
            ));
        }
        if self.report.identity.family == DurableAuditEventFamily::SecurityDecision
            && self.principal_binding.policy_version.is_none()
        {
            return Err(audit_error(
                "durable audit security replay record requires policy version evidence",
            ));
        }
        validate_permissioned_critical_policy_binding(
            self.report.identity.family,
            &self.principal_binding,
            "durable audit replay records",
        )?;
        Ok(())
    }

    pub fn matches_replay_filter(&self, query: &DurableAuditReplayQuery) -> bool {
        if let Some(family) = query.family
            && self.report.identity.family != family
        {
            return false;
        }
        if let Some(trace_id) = query.trace_id
            && self.report.identity.trace_id != trace_id
        {
            return false;
        }
        if let Some(principal_id) = &query.principal_id
            && self.principal_binding.principal_id != *principal_id
        {
            return false;
        }
        if let Some(range) = query.lsn_range
            && !range.contains(self.report.evidence.record_lsn)
            && !range.contains(self.report.evidence.durable_lsn)
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditReplayResult {
    pub evidence: DurableAuditReplayEvidence,
    pub records: Vec<DurableAuditReplayRecord>,
}

fn validate_permissioned_critical_policy_binding(
    family: DurableAuditEventFamily,
    binding: &DurableAuditPrincipalBinding,
    context: &str,
) -> AndromedaResult<()> {
    if !classify_policy_evidence_requirement(family, binding).requires_policy_evidence() {
        return Ok(());
    }

    let Some(policy_version) = binding.policy_version.as_ref() else {
        return Err(audit_error(format!(
            "{context} require policy version evidence for permissioned critical {family:?} records",
        )));
    };
    if !policy_version.has_version_evidence() {
        return Err(audit_error(format!(
            "{context} require non-zero canonical policy version and digest evidence for permissioned critical {family:?} records",
        )));
    }

    Ok(())
}
