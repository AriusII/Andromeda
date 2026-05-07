use super::membership::QuorumMembership;

/// Replication consistency mode: controls write admission and fencing behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    /// Asynchronous: primary can commit without waiting for replica ACKs.
    /// Writes are visible immediately; fencing only triggers on explicit policy.
    Asynchronous,
    /// Synchronous/Quorum: primary must wait for min quorum of replicas to ACK.
    /// Requires quorum to be maintained; write admission blocked if quorum lost.
    QuorumEnforced,
}

impl ReplicationMode {
    pub const fn is_async(&self) -> bool {
        matches!(self, Self::Asynchronous)
    }

    pub const fn is_quorum(&self) -> bool {
        matches!(self, Self::QuorumEnforced)
    }
}

/// Fencing policy: controls when replication failures trigger write blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingPolicy {
    /// Never block writes on replica failure; allow degraded operation.
    Allow,
    /// Block writes if quorum membership is lost.
    BlockOnQuorumLoss,
}

/// Events that trigger fencing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingEvent {
    /// Replica connection lost.
    ReplicaDisconnected,
    /// Replica checksum validation failed.
    ReplicaChecksumMismatch,
    /// Replica reported LSN gap (lost records).
    ReplicaLsnGap,
    /// Unknown/unclassified failure.
    Unknown,
}

/// Result of fencing decision: allow or block new writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingDecision {
    /// Allow new writes to proceed.
    Allow,
    /// Block new writes (wait for quorum recovery or operator intervention).
    Block,
}

impl FencingDecision {
    pub const fn is_allow(self) -> bool {
        matches!(self, Self::Allow)
    }

    pub const fn is_block(self) -> bool {
        matches!(self, Self::Block)
    }
}

/// Pure function: decide whether to allow or block writes based on
/// membership state, LSN state, and fencing policy.
///
/// # Arguments
///
/// * `membership` — Current quorum membership snapshot.
/// * `event` — Fencing event that triggered this decision.
/// * `policy` — Fencing policy (Allow vs BlockOnQuorumLoss).
/// * `replication_mode` — Async vs Quorum mode.
///
/// # Returns
///
/// [`FencingDecision::Allow`] if writes should proceed,
/// [`FencingDecision::Block`] if writes should be held.
///
/// # Determinism
///
/// This function is pure: given the same inputs, it always produces the same
/// output. It is safe to replay and does not depend on time or I/O.
pub fn decide_fencing(
    membership: &QuorumMembership,
    _event: FencingEvent,
    policy: FencingPolicy,
    replication_mode: ReplicationMode,
) -> FencingDecision {
    match policy {
        FencingPolicy::Allow => FencingDecision::Allow,
        FencingPolicy::BlockOnQuorumLoss => {
            // In async mode, always allow (no quorum requirement).
            if replication_mode.is_async() {
                FencingDecision::Allow
            } else if membership.has_quorum() {
                FencingDecision::Allow
            } else {
                FencingDecision::Block
            }
        }
    }
}
