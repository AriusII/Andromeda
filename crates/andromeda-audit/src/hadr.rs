//! HA/DR audit event contracts.

use std::time::SystemTime;

use andromeda_observability::{EventSchemaVersion, TraceId};

use super::{SecurityPolicyVersionEvidence, contains_sensitive_marker};

const CLUSTER_EVENT_REASON_MAX_LEN: usize = 160;
const CLUSTER_EVENT_LABEL_MAX_LEN: usize = 48;
const CLUSTER_EVENT_ALLOWED_QUORUM_SUMMARIES: &[&str] = &[
    "quorum_met",
    "quorum_degraded",
    "quorum_lost",
    "quorum_recovered",
];
const CLUSTER_EVENT_ALLOWED_FENCING_SUMMARIES: &[&str] = &[
    "fencing_not_required",
    "fencing_requested",
    "fencing_executed",
    "fencing_failed",
];
const CLUSTER_EVENT_ALLOWED_DECISIONS: &[&str] =
    &["allow", "reject", "fence", "promote", "recover"];
const CLUSTER_EVENT_ALLOWED_RESULTS: &[&str] = &["success", "rejected", "error"];
const CLUSTER_EVENT_ALLOWED_REJECTION_CODES: &[&str] = &[
    "policy_denied",
    "principal_missing",
    "quorum_not_reached",
    "fencing_required",
    "stale_lsn",
];
const CLUSTER_EVENT_ALLOWED_ERROR_CODES: &[&str] = &[
    "timeout",
    "io_failure",
    "network_partition",
    "consensus_unavailable",
    "unknown",
];

/// Replica health state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplicaHealthState {
    Alive,
    Suspect,
    Dead,
}

impl ReplicaHealthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alive => "alive",
            Self::Suspect => "suspect",
            Self::Dead => "dead",
        }
    }
}

/// Fencing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FencingPolicy {
    ConservativeQuorum,
    OptimisticReplication,
}

impl FencingPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConservativeQuorum => "conservative_quorum",
            Self::OptimisticReplication => "optimistic_replication",
        }
    }
}

/// Fencing trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FencingEvent {
    ReplicaSuspect { replica_id: u64 },
    ReplicaDead { replica_id: u64 },
    WriteAdmissionTimeout,
    OperatorCommand,
}

/// Write-admission fencing decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FencingDecision {
    Allow,
    Block,
}

impl FencingDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
        }
    }
}

/// Promotion eligibility verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromotionEligibility {
    Eligible,
    WalGapTooLarge,
    HealthNotCurrent,
    FencingTokenNotAcquired,
    QuorumNotReached,
}

impl PromotionEligibility {
    pub const fn is_eligible(self) -> bool {
        matches!(self, Self::Eligible)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::WalGapTooLarge => "wal_gap_too_large",
            Self::HealthNotCurrent => "health_not_current",
            Self::FencingTokenNotAcquired => "fencing_token_not_acquired",
            Self::QuorumNotReached => "quorum_not_reached",
        }
    }
}

/// Promotion completion status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionCompletion {
    Success { new_epoch: u64 },
    Failed { reason: String },
}

/// Quorum member role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuorumRole {
    Primary,
    SyncReplica,
    AsyncReplica,
}

impl QuorumRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::SyncReplica => "sync_replica",
            Self::AsyncReplica => "async_replica",
        }
    }
}

/// HA/DR audit event types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HadrAuditEvent {
    /// Replica health state changed.
    ReplicaHealthTransition {
        replica_id: u64,
        from: ReplicaHealthState,
        to: ReplicaHealthState,
    },

    /// Fencing decision made.
    FencingDecision {
        policy: FencingPolicy,
        event: FencingEvent,
        decision: FencingDecision,
    },

    /// WAL segment shipped from primary to replica.
    WalSegmentShipped {
        segment_id: u64,
        replica_id: u64,
        lsn_range: (u64, u64),
        checksum: u64,
    },

    /// Replica promotion eligibility computed.
    PromotionEligibilityComputed {
        replica_id: u64,
        eligibility: PromotionEligibility,
    },

    /// Replica promoted to primary.
    PromotionExecuted {
        promoted_replica_id: u64,
        new_epoch: u64,
    },

    /// Quorum membership changed.
    MembershipChange {
        old_epoch: u64,
        new_epoch: u64,
        members: Vec<(u64, QuorumRole)>,
    },
    /// Versioned cluster decision evidence for HA/DR critical paths.
    ClusterEventTraceV0 {
        schema_version: EventSchemaVersion,
        security_sensitive: bool,
        policy_version: Option<SecurityPolicyVersionEvidence>,
        principal_id: Option<String>,
        epoch: u64,
        node_id: u64,
        quorum_summary: String,
        fencing_summary: String,
        last_valid_lsn: u64,
        decision: String,
        result: String,
        rejection_code: Option<String>,
        error_code: Option<String>,
        reason: String,
        decision_trace_id: Option<TraceId>,
        recovery_report_id: Option<u64>,
    },
}

impl HadrAuditEvent {
    /// Stable event type label.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ReplicaHealthTransition { .. } => "replica_health_transition",
            Self::FencingDecision { .. } => "fencing_decision",
            Self::WalSegmentShipped { .. } => "wal_segment_shipped",
            Self::PromotionEligibilityComputed { .. } => "promotion_eligibility_computed",
            Self::PromotionExecuted { .. } => "promotion_executed",
            Self::MembershipChange { .. } => "membership_change",
            Self::ClusterEventTraceV0 { .. } => "cluster_event_trace_v0",
        }
    }

    /// Replica id when the event is replica-scoped.
    pub fn affected_replica_id(&self) -> Option<u64> {
        match self {
            Self::ReplicaHealthTransition { replica_id, .. } => Some(*replica_id),
            Self::FencingDecision { .. } => None,
            Self::WalSegmentShipped { replica_id, .. } => Some(*replica_id),
            Self::PromotionEligibilityComputed { replica_id, .. } => Some(*replica_id),
            Self::PromotionExecuted {
                promoted_replica_id,
                ..
            } => Some(*promoted_replica_id),
            Self::MembershipChange { .. } => None,
            Self::ClusterEventTraceV0 { node_id, .. } => Some(*node_id),
        }
    }

    pub fn validates_contract_with_principal(&self, principal: &str) -> bool {
        match self {
            Self::ClusterEventTraceV0 {
                schema_version,
                security_sensitive,
                policy_version,
                principal_id,
                epoch,
                node_id,
                quorum_summary,
                fencing_summary,
                last_valid_lsn,
                decision,
                result,
                rejection_code,
                error_code,
                reason,
                decision_trace_id,
                recovery_report_id,
            } => {
                schema_version.is_v0()
                    && *epoch > 0
                    && *node_id > 0
                    && *last_valid_lsn > 0
                    && label_is_allowed(quorum_summary, CLUSTER_EVENT_ALLOWED_QUORUM_SUMMARIES)
                    && label_is_allowed(fencing_summary, CLUSTER_EVENT_ALLOWED_FENCING_SUMMARIES)
                    && label_is_allowed(decision, CLUSTER_EVENT_ALLOWED_DECISIONS)
                    && label_is_allowed(result, CLUSTER_EVENT_ALLOWED_RESULTS)
                    && reason_is_valid(reason)
                    && principal_id_is_valid(principal_id)
                    && decision_trace_id.is_none_or(|trace_id| !trace_id.is_zero())
                    && recovery_report_id.is_none_or(|report_id| report_id > 0)
                    && validate_result_consistency(result, rejection_code, error_code)
                    && validate_security_binding(
                        *security_sensitive,
                        policy_version.as_ref(),
                        principal,
                        principal_id.as_deref(),
                    )
            },
            _ => true,
        }
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        match self {
            Self::ClusterEventTraceV0 {
                principal_id,
                reason,
                rejection_code,
                error_code,
                ..
            } => {
                principal_id
                    .as_deref()
                    .is_some_and(contains_sensitive_marker)
                    || contains_sensitive_marker(reason)
                    || rejection_code
                        .as_deref()
                        .is_some_and(contains_sensitive_marker)
                    || error_code.as_deref().is_some_and(contains_sensitive_marker)
            },
            _ => false,
        }
    }
}

/// Audit trace for a single HA/DR event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrAuditTrace {
    pub trace_id: TraceId,
    pub principal: String,
    pub event: HadrAuditEvent,
    pub timestamp_ms: u64,
    pub sequence_number: u64,
}

impl HadrAuditTrace {
    pub fn new(
        trace_id: TraceId,
        principal: impl Into<String>,
        event: HadrAuditEvent,
        timestamp: SystemTime,
        sequence_number: u64,
    ) -> Self {
        let timestamp_ms = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            trace_id,
            principal: principal.into(),
            event,
            timestamp_ms,
            sequence_number,
        }
    }

    /// Validate required identity and ordering evidence.
    pub fn validate(&self) -> bool {
        !self.trace_id.is_zero()
            && !self.principal.trim().is_empty()
            && !contains_sensitive_marker(&self.principal)
            && self.sequence_number > 0
            && self
                .event
                .validates_contract_with_principal(&self.principal)
            && !self.event.contains_sensitive_evidence()
    }
}

fn label_is_allowed(value: &str, allowed: &[&str]) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= CLUSTER_EVENT_LABEL_MAX_LEN
        && !contains_sensitive_marker(trimmed)
        && allowed.iter().any(|candidate| *candidate == trimmed)
}

fn reason_is_valid(reason: &str) -> bool {
    let trimmed = reason.trim();
    !trimmed.is_empty()
        && trimmed.len() <= CLUSTER_EVENT_REASON_MAX_LEN
        && !contains_sensitive_marker(trimmed)
}

fn principal_id_is_valid(principal_id: &Option<String>) -> bool {
    principal_id.as_ref().is_none_or(|principal_id| {
        let trimmed = principal_id.trim();
        !trimmed.is_empty()
            && trimmed.len() <= CLUSTER_EVENT_LABEL_MAX_LEN
            && !contains_sensitive_marker(trimmed)
    })
}

fn validate_result_consistency(
    result: &str,
    rejection_code: &Option<String>,
    error_code: &Option<String>,
) -> bool {
    let result = result.trim();
    if !label_is_allowed(result, CLUSTER_EVENT_ALLOWED_RESULTS) {
        return false;
    }

    let rejection_is_valid = rejection_code
        .as_deref()
        .is_none_or(|code| label_is_allowed(code, CLUSTER_EVENT_ALLOWED_REJECTION_CODES));
    let error_is_valid = error_code
        .as_deref()
        .is_none_or(|code| label_is_allowed(code, CLUSTER_EVENT_ALLOWED_ERROR_CODES));
    if !rejection_is_valid || !error_is_valid {
        return false;
    }

    match result {
        "success" => rejection_code.is_none() && error_code.is_none(),
        "rejected" => rejection_code.is_some() && error_code.is_none(),
        "error" => error_code.is_some(),
        _ => false,
    }
}

fn validate_security_binding(
    security_sensitive: bool,
    policy_version: Option<&SecurityPolicyVersionEvidence>,
    trace_principal: &str,
    principal_id: Option<&str>,
) -> bool {
    let trace_principal = trace_principal.trim();
    if trace_principal.is_empty() || contains_sensitive_marker(trace_principal) {
        return false;
    }

    let Some(principal_id) = principal_id else {
        return false;
    };
    let principal_id = principal_id.trim();
    if principal_id != trace_principal
        || principal_id.is_empty()
        || principal_id.len() > CLUSTER_EVENT_LABEL_MAX_LEN
        || contains_sensitive_marker(principal_id)
    {
        return false;
    }

    if !security_sensitive {
        return policy_version.is_none_or(SecurityPolicyVersionEvidence::has_version_evidence);
    }

    policy_version.is_some_and(SecurityPolicyVersionEvidence::has_version_evidence)
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    fn policy_v1() -> SecurityPolicyVersionEvidence {
        SecurityPolicyVersionEvidence::new(
            1,
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("policy digest must be canonical")
    }

    fn valid_cluster_event() -> HadrAuditEvent {
        HadrAuditEvent::ClusterEventTraceV0 {
            schema_version: EventSchemaVersion::V0,
            security_sensitive: true,
            policy_version: Some(policy_v1()),
            principal_id: Some("operator:cluster".to_string()),
            epoch: 10,
            node_id: 1,
            quorum_summary: "quorum_met".to_string(),
            fencing_summary: "fencing_not_required".to_string(),
            last_valid_lsn: 42,
            decision: "allow".to_string(),
            result: "success".to_string(),
            rejection_code: None,
            error_code: None,
            reason: "cluster admission accepted".to_string(),
            decision_trace_id: Some(TraceId::new(5)),
            recovery_report_id: Some(6),
        }
    }

    #[test]
    fn cluster_event_trace_v0_validate_accepts_required_fields() {
        let trace = HadrAuditTrace::new(
            TraceId::new(9),
            "operator:cluster",
            valid_cluster_event(),
            SystemTime::now(),
            1,
        );

        assert!(trace.validate());
    }

    #[test]
    fn cluster_event_trace_v0_validate_rejects_missing_security_binding() {
        let trace = HadrAuditTrace::new(
            TraceId::new(9),
            "operator:cluster",
            HadrAuditEvent::ClusterEventTraceV0 {
                schema_version: EventSchemaVersion::V0,
                security_sensitive: true,
                policy_version: None,
                principal_id: None,
                epoch: 10,
                node_id: 1,
                quorum_summary: "quorum_met".to_string(),
                fencing_summary: "fencing_not_required".to_string(),
                last_valid_lsn: 42,
                decision: "allow".to_string(),
                result: "success".to_string(),
                rejection_code: None,
                error_code: None,
                reason: "cluster admission accepted".to_string(),
                decision_trace_id: Some(TraceId::new(5)),
                recovery_report_id: Some(6),
            },
            SystemTime::now(),
            1,
        );

        assert!(!trace.validate());
    }

    #[test]
    fn cluster_event_trace_v0_validate_rejects_principal_mismatch() {
        let trace = HadrAuditTrace::new(
            TraceId::new(9),
            "operator:cluster",
            HadrAuditEvent::ClusterEventTraceV0 {
                schema_version: EventSchemaVersion::V0,
                security_sensitive: true,
                policy_version: Some(policy_v1()),
                principal_id: Some("operator:other".to_string()),
                epoch: 10,
                node_id: 1,
                quorum_summary: "quorum_met".to_string(),
                fencing_summary: "fencing_not_required".to_string(),
                last_valid_lsn: 42,
                decision: "allow".to_string(),
                result: "success".to_string(),
                rejection_code: None,
                error_code: None,
                reason: "cluster admission accepted".to_string(),
                decision_trace_id: Some(TraceId::new(5)),
                recovery_report_id: Some(6),
            },
            SystemTime::now(),
            1,
        );

        assert!(!trace.validate());
    }

    #[test]
    fn cluster_event_trace_v0_validate_rejects_principal_downgrade_without_binding() {
        let trace = HadrAuditTrace::new(
            TraceId::new(9),
            "operator:cluster",
            HadrAuditEvent::ClusterEventTraceV0 {
                schema_version: EventSchemaVersion::V0,
                security_sensitive: false,
                policy_version: None,
                principal_id: None,
                epoch: 10,
                node_id: 1,
                quorum_summary: "quorum_met".to_string(),
                fencing_summary: "fencing_not_required".to_string(),
                last_valid_lsn: 42,
                decision: "allow".to_string(),
                result: "success".to_string(),
                rejection_code: None,
                error_code: None,
                reason: "cluster admission accepted".to_string(),
                decision_trace_id: Some(TraceId::new(5)),
                recovery_report_id: Some(6),
            },
            SystemTime::now(),
            1,
        );

        assert!(!trace.validate());
    }

    #[test]
    fn cluster_event_trace_v0_validate_rejects_secret_reason() {
        let trace = HadrAuditTrace::new(
            TraceId::new(9),
            "operator:cluster",
            HadrAuditEvent::ClusterEventTraceV0 {
                schema_version: EventSchemaVersion::V0,
                security_sensitive: true,
                policy_version: Some(policy_v1()),
                principal_id: Some("operator:cluster".to_string()),
                epoch: 10,
                node_id: 1,
                quorum_summary: "quorum_met".to_string(),
                fencing_summary: "fencing_not_required".to_string(),
                last_valid_lsn: 42,
                decision: "allow".to_string(),
                result: "success".to_string(),
                rejection_code: None,
                error_code: None,
                reason: "token=secret".to_string(),
                decision_trace_id: Some(TraceId::new(5)),
                recovery_report_id: Some(6),
            },
            SystemTime::now(),
            1,
        );

        assert!(!trace.validate());
    }
}
