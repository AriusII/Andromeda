//! HA/DR audit event contracts.

use std::time::SystemTime;

use andromeda_observability::TraceId;

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
        !self.trace_id.is_zero() && !self.principal.is_empty() && self.sequence_number > 0
    }
}
