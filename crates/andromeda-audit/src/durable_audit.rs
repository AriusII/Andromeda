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

    pub const fn requires_complete_permission_binding(self) -> bool {
        matches!(
            self,
            Self::SecurityDecision
                | Self::AdminDecision
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
pub struct DurableAuditRetentionPolicy {
    pub min_record_lsn: Option<u64>,
    pub min_durable_lsn: Option<u64>,
    pub preserve_forensic_hold: bool,
    pub retain_boundaries: Vec<DurableAuditRetentionBoundary>,
}

impl DurableAuditRetentionPolicy {
    pub fn retain_record_lsn_at_or_after(min_record_lsn: u64) -> Self {
        Self {
            min_record_lsn: Some(min_record_lsn),
            ..Self::default()
        }
    }

    pub fn with_forensic_hold_preserved(mut self, preserve_forensic_hold: bool) -> Self {
        self.preserve_forensic_hold = preserve_forensic_hold;
        self
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.min_record_lsn == Some(0) {
            return Err(audit_error(
                "durable audit retention policy min_record_lsn must be non-zero when present",
            ));
        }
        if self.min_durable_lsn == Some(0) {
            return Err(audit_error(
                "durable audit retention policy min_durable_lsn must be non-zero when present",
            ));
        }
        Ok(())
    }

    pub fn retains(&self, record: &DurableAuditReplayRecord) -> bool {
        if self.has_no_expiry_criteria() {
            return true;
        }
        if self.preserve_forensic_hold
            && record.report.retention == DurableAuditRetentionBoundary::ForensicHold
        {
            return true;
        }
        if self.retain_boundaries.contains(&record.report.retention) {
            return true;
        }
        if self
            .min_record_lsn
            .is_some_and(|min_lsn| record.report.evidence.record_lsn >= min_lsn)
        {
            return true;
        }
        self.min_durable_lsn
            .is_some_and(|min_lsn| record.report.evidence.durable_lsn >= min_lsn)
    }

    fn has_no_expiry_criteria(&self) -> bool {
        self.min_record_lsn.is_none()
            && self.min_durable_lsn.is_none()
            && self.retain_boundaries.is_empty()
    }
}

impl Default for DurableAuditRetentionPolicy {
    fn default() -> Self {
        Self {
            min_record_lsn: None,
            min_durable_lsn: None,
            preserve_forensic_hold: true,
            retain_boundaries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditCompactionReport {
    pub records_scanned: usize,
    pub records_retained: usize,
    pub records_expired: usize,
    pub first_retained_lsn: Option<u64>,
    pub last_retained_lsn: Option<u64>,
    pub retained_checksum_evidence: u64,
}

impl DurableAuditCompactionReport {
    pub fn from_records(
        records_scanned: usize,
        retained: &[DurableAuditReplayRecord],
    ) -> AndromedaResult<Self> {
        let records_retained = retained.len();
        if records_retained > records_scanned {
            return Err(audit_error(
                "durable audit compaction cannot retain more records than it scanned",
            ));
        }

        let mut retained_checksum_evidence = 0u64;
        let mut previous_lsn = 0u64;
        for record in retained {
            record.validate()?;
            let lsn = record.report.evidence.record_lsn;
            if lsn <= previous_lsn {
                return Err(audit_error(
                    "durable audit compaction retained records must remain LSN ordered",
                ));
            }
            previous_lsn = lsn;
            retained_checksum_evidence = retained_checksum_evidence
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(record.report.evidence.checksum)
                .wrapping_add(lsn);
        }

        Ok(Self {
            records_scanned,
            records_retained,
            records_expired: records_scanned.saturating_sub(records_retained),
            first_retained_lsn: retained
                .first()
                .map(|record| record.report.evidence.record_lsn),
            last_retained_lsn: retained
                .last()
                .map(|record| record.report.evidence.record_lsn),
            retained_checksum_evidence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditWalSegmentArchiveProof {
    pub archive_id: String,
    pub first_lsn: u64,
    pub last_lsn: u64,
    pub checksum: u64,
}

impl DurableAuditWalSegmentArchiveProof {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.archive_id.trim().is_empty() {
            return Err(audit_error(
                "durable audit WAL archive proof requires non-empty archive_id",
            ));
        }
        if contains_sensitive_marker(&self.archive_id) {
            return Err(audit_error(
                "durable audit WAL archive proof must not contain secret evidence",
            ));
        }
        if self.first_lsn == 0 || self.last_lsn == 0 || self.first_lsn > self.last_lsn {
            return Err(audit_error(
                "durable audit WAL archive proof requires a non-zero ordered LSN range",
            ));
        }
        if self.checksum == 0 {
            return Err(audit_error(
                "durable audit WAL archive proof requires non-zero checksum evidence",
            ));
        }
        Ok(())
    }

    pub const fn covers(&self, evidence: DurableAuditWalEvidence) -> bool {
        self.first_lsn <= evidence.record_lsn && evidence.durable_lsn <= self.last_lsn
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableAuditPruneBlockReason {
    PolicyRetainsRecord,
    ForensicHold,
    SecurityPolicy,
    CatalogVersionRetentionBoundary,
    MissingWalSegmentArchiveProof,
    WalSegmentArchiveProofMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditPruneEvidence {
    pub identity: DurableAuditRecordIdentity,
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub record_checksum: u64,
    pub retention: DurableAuditRetentionBoundary,
    pub archive_proof: Option<DurableAuditWalSegmentArchiveProof>,
    pub prune_allowed: bool,
    pub blocked_by: Option<DurableAuditPruneBlockReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditRetentionManager {
    pub policy: DurableAuditRetentionPolicy,
}

impl DurableAuditRetentionManager {
    pub fn new(policy: DurableAuditRetentionPolicy) -> AndromedaResult<Self> {
        policy.validate()?;
        Ok(Self { policy })
    }

    pub fn evaluate_prune(
        &self,
        record: &DurableAuditReplayRecord,
        archive_proof: Option<DurableAuditWalSegmentArchiveProof>,
    ) -> AndromedaResult<DurableAuditPruneEvidence> {
        record.validate()?;
        self.policy.validate()?;
        if let Some(proof) = &archive_proof {
            proof.validate()?;
        }

        let block = match record.report.retention {
            DurableAuditRetentionBoundary::ForensicHold => {
                Some(DurableAuditPruneBlockReason::ForensicHold)
            },
            DurableAuditRetentionBoundary::SecurityPolicy => {
                Some(DurableAuditPruneBlockReason::SecurityPolicy)
            },
            _ if self.policy.retains(record) => {
                Some(DurableAuditPruneBlockReason::PolicyRetainsRecord)
            },
            DurableAuditRetentionBoundary::WalSegment => match &archive_proof {
                Some(proof)
                    if proof.covers(record.report.evidence)
                        && proof.checksum == record.report.evidence.checksum =>
                {
                    None
                },
                Some(_) => Some(DurableAuditPruneBlockReason::WalSegmentArchiveProofMismatch),
                None => Some(DurableAuditPruneBlockReason::MissingWalSegmentArchiveProof),
            },
            DurableAuditRetentionBoundary::CatalogVersion => {
                Some(DurableAuditPruneBlockReason::CatalogVersionRetentionBoundary)
            },
        };

        Ok(DurableAuditPruneEvidence {
            identity: record.report.identity,
            record_lsn: record.report.evidence.record_lsn,
            durable_lsn: record.report.evidence.durable_lsn,
            record_checksum: record.report.evidence.checksum,
            retention: record.report.retention,
            archive_proof,
            prune_allowed: block.is_none(),
            blocked_by: block,
        })
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditAppendRecord {
    pub identity: DurableAuditRecordIdentity,
    pub principal_binding: DurableAuditPrincipalBinding,
    pub retention: DurableAuditRetentionBoundary,
    pub replay_behavior: DurableAuditReplayBehavior,
    pub event_kind: String,
}

impl DurableAuditAppendRecord {
    pub fn new(
        identity: DurableAuditRecordIdentity,
        principal_binding: DurableAuditPrincipalBinding,
        retention: DurableAuditRetentionBoundary,
        replay_behavior: DurableAuditReplayBehavior,
        event_kind: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let record = Self {
            identity,
            principal_binding,
            retention,
            replay_behavior,
            event_kind: event_kind.into(),
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.identity.validate()?;
        self.principal_binding.validate()?;
        if self.event_kind.trim().is_empty() {
            return Err(audit_error(
                "durable audit append record requires event kind evidence",
            ));
        }
        if contains_sensitive_marker(&self.event_kind) {
            return Err(audit_error(
                "durable audit append event kind must not contain secret evidence",
            ));
        }
        validate_complete_permission_binding(
            self.identity.family,
            &self.principal_binding,
            "durable audit append records",
        )?;
        validate_permissioned_critical_policy_binding(
            self.identity.family,
            &self.principal_binding,
            "durable audit append records",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditSinkFailure {
    pub kind: DurableAuditFailureKind,
    pub identity: Option<DurableAuditRecordIdentity>,
    pub reason: String,
}

impl DurableAuditSinkFailure {
    pub fn new(
        kind: DurableAuditFailureKind,
        identity: Option<DurableAuditRecordIdentity>,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let failure = Self {
            kind,
            identity,
            reason: reason.into(),
        };
        failure.validate()?;
        Ok(failure)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if let Some(identity) = self.identity {
            identity.validate()?;
        }
        if self.reason.trim().is_empty() {
            return Err(audit_error(
                "durable audit sink failure requires non-empty reason evidence",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(audit_error(
                "durable audit sink failure reason must not contain secret evidence",
            ));
        }
        Ok(())
    }

    pub fn requires_fail_closed(&self) -> bool {
        self.kind.requires_fail_closed()
    }
}

pub type DurableAuditSinkResult<T> = Result<T, DurableAuditSinkFailure>;

pub(crate) fn sink_failure(
    kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
    reason: impl Into<String>,
) -> DurableAuditSinkFailure {
    let identity = identity.filter(|identity| identity.validate().is_ok());
    let reason = reason.into();
    let reason = if reason.trim().is_empty() || contains_sensitive_marker(&reason) {
        "durable audit sink operation failed".to_string()
    } else {
        reason
    };

    DurableAuditSinkFailure::new(kind, identity, reason).unwrap_or(DurableAuditSinkFailure {
        kind,
        identity: None,
        reason: "durable audit sink operation failed".to_string(),
    })
}

pub type DurableAuditReplayLsnRange = andromeda_observability::TraceQueryLsnRange;

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
        validate_complete_permission_binding(
            self.report.identity.family,
            &self.principal_binding,
            "durable audit replay records",
        )?;
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

fn validate_complete_permission_binding(
    family: DurableAuditEventFamily,
    binding: &DurableAuditPrincipalBinding,
    context: &str,
) -> AndromedaResult<()> {
    if !family.requires_complete_permission_binding() {
        return Ok(());
    }

    if binding.certificate_fingerprint.is_none()
        || binding.surface.is_none()
        || binding.permission.is_none()
    {
        return Err(audit_error(format!(
            "{context} require certificate, surface, and permission evidence for permissioned critical {family:?} records",
        )));
    }

    Ok(())
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
