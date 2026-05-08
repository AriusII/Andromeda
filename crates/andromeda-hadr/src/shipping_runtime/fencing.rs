/// Fencing decision at the primary: should we block commits if a replica is lost?
///
/// The quorum policy determines replication mode:
/// - Single replica: async (continue writing even if replica is unreachable).
/// - 2+ replicas: quorum (block writes until a quorum of replicas acknowledge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingPolicy {
    /// Asynchronous: continue writing regardless of replica ack. Used when replica count = 1.
    Asynchronous,
    /// Quorum-enforced: wait for a majority of replicas to acknowledge before visibility.
    /// Used when replica count >= 2.
    QuorumEnforced,
}

impl FencingPolicy {
    /// Select fencing policy based on replica count.
    pub fn for_replica_count(replica_count: usize) -> Self {
        if replica_count >= 2 {
            FencingPolicy::QuorumEnforced
        } else {
            FencingPolicy::Asynchronous
        }
    }
}

/// Fencing event: what happened when we tried to ship to a replica and it failed?
///
/// The primary emits these as trace events. They inform the promotion/demotion flow
/// and may trigger operational alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingEvent {
    /// Replica connection lost. No ack received for the last segment sent.
    ReplicaConnectionLost,
    /// Replica returned a checksum mismatch for a segment we sent.
    ReplicaChecksumMismatch,
    /// Replica reported a gap in the LSN chain. Possible data corruption.
    ReplicaChainGap,
    /// Replica returned an error we don't understand. Safe to assume it's fenced.
    ReplicaUnknownError,
}

impl FencingEvent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReplicaConnectionLost => "replica connection lost",
            Self::ReplicaChecksumMismatch => "replica checksum mismatch",
            Self::ReplicaChainGap => "replica chain gap detected",
            Self::ReplicaUnknownError => "replica unknown error",
        }
    }
}

/// Fencing decision: given a fencing event and the quorum policy, decide what to do.
///
/// Returns `true` if we should block transaction visibility (synchronous mode).
/// Returns `false` if we should continue (asynchronous mode).
pub fn decide_fencing(_event: FencingEvent, policy: FencingPolicy) -> bool {
    match policy {
        FencingPolicy::Asynchronous => {
            // Async mode: continue writing even if replica fails.
            false
        },
        FencingPolicy::QuorumEnforced => {
            // Quorum mode: block visibility on any replica failure.
            // (The actual quorum algorithm is in F3; this is the decision point.)
            true
        },
    }
}
