//! HA/DR operational audit event types.
//!
//! This module defines the audit event taxonomy for high-availability and disaster-recovery
//! operations. Each event captures a critical decision point or state transition in the
//! HA/DR lifecycle: replica health, fencing decisions, WAL shipping, promotion eligibility,
//! and quorum membership changes.
//!
//! ## Audit Principles
//!
//! - **Immutability**: Once emitted, an audit event is immutable and append-only.
//! - **Traceability**: Every event carries a `TraceId` for forensic correlation.
//! - **Principal Binding**: HA/DR events carry an operator or system principal to distinguish
//!   automatic recovery from explicit operator intervention.
//! - **Determinism**: Reason strings are non-empty and machine-parseable where possible.
//! - **No Silent Drops**: All audit events are validated before emission; rejections are observed.

use std::time::SystemTime;

use crate::TraceId;

/// Replica health state as observed by quorum membership manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplicaHealthState {
    /// Replica is responsive and current with commits.
    Alive,
    /// Replica is non-responsive but not yet evicted; communication timeout triggered.
    Suspect,
    /// Replica has been evicted from quorum due to sustained unavailability.
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

/// Fencing policy decision: allow or block writes at primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FencingPolicy {
    /// Conservative: Require majority quorum before write admission.
    ConservativeQuorum,
    /// Optimistic: Allow writes if any replica has confirmed receipt.
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

/// Event that triggered fencing evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FencingEvent {
    /// Replica became Suspect; evaluate quorum impact.
    ReplicaSuspect { replica_id: u64 },
    /// Replica became Dead; evaluate quorum impact.
    ReplicaDead { replica_id: u64 },
    /// Timeout during write admission; re-evaluate quorum.
    WriteAdmissionTimeout,
    /// Operator explicitly commanded fencing review.
    OperatorCommand,
}

/// Fencing decision: allow writes to proceed or block until quorum recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FencingDecision {
    /// Writes may proceed; quorum criteria satisfied.
    Allow,
    /// Writes must be blocked; quorum criteria not satisfied.
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

/// Promotion eligibility criteria verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromotionEligibility {
    /// Replica is eligible for promotion (all criteria met).
    Eligible,
    /// Replica is not eligible; WAL gap too large.
    WalGapTooLarge,
    /// Replica is not eligible; health state not current.
    HealthNotCurrent,
    /// Replica is not eligible; fencing token not acquired.
    FencingTokenNotAcquired,
    /// Replica is not eligible; quorum consensus not reached.
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

/// Promotion completion status: success or failure with reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionCompletion {
    /// Promotion succeeded; new epoch established.
    Success { new_epoch: u64 },
    /// Promotion failed; reason describes root cause.
    Failed { reason: String },
}

/// Quorum role assignment for a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuorumRole {
    /// Member is the current primary.
    Primary,
    /// Member is a synchronous replica.
    SyncReplica,
    /// Member is an asynchronous replica.
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

/// HA/DR audit event types. Each event is immutable once created and represents
/// a critical decision point or state transition in HA/DR operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HadrAuditEvent {
    /// Replica health state changed (e.g., Alive → Suspect → Dead).
    ///
    /// Emitted by: `andromeda-storage::hadr::quorum_runtime`
    /// Triggering condition: Health check timeout, communication restored, or explicit operator command.
    ReplicaHealthTransition {
        /// Unique replica identifier.
        replica_id: u64,
        /// Previous health state.
        from: ReplicaHealthState,
        /// New health state.
        to: ReplicaHealthState,
    },

    /// Fencing decision made (allow or block writes).
    ///
    /// Emitted by: `andromeda-storage::hadr::quorum_runtime::decide_fencing`
    /// Triggering condition: Replica health change, timeout event, or operator command.
    FencingDecision {
        /// Fencing policy active (ConservativeQuorum or OptimisticReplication).
        policy: FencingPolicy,
        /// Event that triggered fencing evaluation.
        event: FencingEvent,
        /// Decision: Allow or Block.
        decision: FencingDecision,
    },

    /// WAL segment shipped from primary to replica.
    ///
    /// Emitted by: `andromeda-storage::write_ahead_log::shipping_runtime`
    /// Triggering condition: WAL segment batch shipped and durably received by replica.
    WalSegmentShipped {
        /// Unique segment identifier (monotonically increasing).
        segment_id: u64,
        /// Replica that received this segment.
        replica_id: u64,
        /// LSN range of shipped segment [start, end).
        lsn_range: (u64, u64),
        /// Deterministic CRC32 checksum of segment contents.
        checksum: u64,
    },

    /// Replica promotion eligibility computed.
    ///
    /// Emitted by: `andromeda-storage::hadr::promotion_boundary`
    /// Triggering condition: Operator requests eligibility check or automatic recovery triggers assessment.
    PromotionEligibilityComputed {
        /// Candidate replica for promotion.
        replica_id: u64,
        /// Eligibility verdict: Eligible or reason for ineligibility.
        eligibility: PromotionEligibility,
    },

    /// Replica promoted to primary (after orchestration completes).
    ///
    /// Emitted by: (Async phases in F6+) Not emitted in V0.5.
    /// Triggering condition: Promotion orchestration succeeds.
    PromotionExecuted {
        /// Replica promoted to primary.
        promoted_replica_id: u64,
        /// New epoch after promotion.
        new_epoch: u64,
    },

    /// Quorum membership changed (members joined, evicted, or reassigned).
    ///
    /// Emitted by: `andromeda-storage::hadr::quorum_runtime`
    /// Triggering condition: Health change triggers rebalancing, operator reconfigures, or new member joins.
    MembershipChange {
        /// Epoch before membership change.
        old_epoch: u64,
        /// Epoch after membership change (incremented).
        new_epoch: u64,
        /// New quorum member list with roles.
        members: Vec<(u64, QuorumRole)>,
    },
}

impl HadrAuditEvent {
    /// Human-readable event type for logging and audit reporting.
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

    /// Deterministic ordering for replica events (for audit timeline consistency).
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

/// Immutable audit trace for a single HA/DR event.
///
/// Binds a HA/DR operational event to:
/// - A unique `trace_id` for forensic correlation.
/// - A `principal` (operator or system) for audit accountability.
/// - A `timestamp` for event sequencing.
/// - A `sequence_number` for ordering within a trace context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrAuditTrace {
    /// Unique trace ID for forensic correlation across HA/DR operations.
    pub trace_id: TraceId,

    /// Principal who triggered or authorized this event (e.g., operator or recovery_system).
    pub principal: String,

    /// The HA/DR audit event itself.
    pub event: HadrAuditEvent,

    /// Timestamp when event was observed (SystemTime in milliseconds since Unix epoch).
    pub timestamp_ms: u64,

    /// Monotonically increasing sequence number for ordering within a single trace context.
    /// Used to establish causal ordering even if multiple threads emit events concurrently.
    pub sequence_number: u64,
}

impl HadrAuditTrace {
    /// Construct a new HA/DR audit trace.
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

    /// Check that trace has valid evidence for audit acceptance.
    pub fn validate(&self) -> bool {
        !self.trace_id.is_zero() && !self.principal.is_empty() && self.sequence_number > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replica_health_states_are_distinct() {
        assert_ne!(ReplicaHealthState::Alive, ReplicaHealthState::Suspect);
        assert_ne!(ReplicaHealthState::Suspect, ReplicaHealthState::Dead);
        assert_ne!(ReplicaHealthState::Alive, ReplicaHealthState::Dead);
    }

    #[test]
    fn fencing_policy_labels_are_consistent() {
        assert_eq!(
            FencingPolicy::ConservativeQuorum.as_str(),
            "conservative_quorum"
        );
        assert_eq!(
            FencingPolicy::OptimisticReplication.as_str(),
            "optimistic_replication"
        );
    }

    #[test]
    fn promotion_eligibility_eligible_predicate_is_accurate() {
        assert!(PromotionEligibility::Eligible.is_eligible());
        assert!(!PromotionEligibility::WalGapTooLarge.is_eligible());
        assert!(!PromotionEligibility::HealthNotCurrent.is_eligible());
        assert!(!PromotionEligibility::FencingTokenNotAcquired.is_eligible());
        assert!(!PromotionEligibility::QuorumNotReached.is_eligible());
    }

    #[test]
    fn hadr_audit_event_type_labels_are_correct() {
        let event = HadrAuditEvent::ReplicaHealthTransition {
            replica_id: 1,
            from: ReplicaHealthState::Alive,
            to: ReplicaHealthState::Suspect,
        };
        assert_eq!(event.event_type(), "replica_health_transition");

        let event = HadrAuditEvent::FencingDecision {
            policy: FencingPolicy::ConservativeQuorum,
            event: FencingEvent::ReplicaSuspect { replica_id: 1 },
            decision: FencingDecision::Allow,
        };
        assert_eq!(event.event_type(), "fencing_decision");
    }

    #[test]
    fn hadr_audit_trace_validates_required_fields() {
        let trace = HadrAuditTrace::new(
            TraceId::new(0), // Invalid: zero trace ID
            "operator:alice",
            HadrAuditEvent::ReplicaHealthTransition {
                replica_id: 1,
                from: ReplicaHealthState::Alive,
                to: ReplicaHealthState::Suspect,
            },
            SystemTime::now(),
            1,
        );
        assert!(!trace.validate()); // Should fail due to zero trace ID

        let trace = HadrAuditTrace::new(
            TraceId::new(123),
            "", // Invalid: empty principal
            HadrAuditEvent::ReplicaHealthTransition {
                replica_id: 1,
                from: ReplicaHealthState::Alive,
                to: ReplicaHealthState::Suspect,
            },
            SystemTime::now(),
            1,
        );
        assert!(!trace.validate()); // Should fail due to empty principal

        let trace = HadrAuditTrace::new(
            TraceId::new(123),
            "operator:alice",
            HadrAuditEvent::ReplicaHealthTransition {
                replica_id: 1,
                from: ReplicaHealthState::Alive,
                to: ReplicaHealthState::Suspect,
            },
            SystemTime::now(),
            0, // Invalid: zero sequence number
        );
        assert!(!trace.validate()); // Should fail due to zero sequence number
    }

    #[test]
    fn hadr_audit_trace_captures_timestamp_in_milliseconds() {
        let now = SystemTime::now();
        let trace = HadrAuditTrace::new(
            TraceId::new(123),
            "system:recovery",
            HadrAuditEvent::ReplicaHealthTransition {
                replica_id: 1,
                from: ReplicaHealthState::Alive,
                to: ReplicaHealthState::Suspect,
            },
            now,
            1,
        );

        // Verify timestamp is in reasonable range (not zero, recent)
        assert!(trace.timestamp_ms > 0);
        assert!(trace.timestamp_ms < u64::MAX);
    }
}
