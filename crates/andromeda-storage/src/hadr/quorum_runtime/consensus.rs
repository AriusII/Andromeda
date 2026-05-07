use super::majority_quorum_size;

/// Quorum consensus decision for write admission.
///
/// Requires that the primary has received acknowledgment from at least
/// `min_quorum_acks` replicas before the write becomes visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuorumConsensus {
    /// Minimum number of alive replica ACKs required before write is visible.
    /// In async mode, this is 0 (writes visible immediately).
    /// In quorum mode, this is typically the membership's quorum_size.
    pub min_quorum_acks: usize,
}

impl QuorumConsensus {
    pub const fn new(min_quorum_acks: usize) -> Self {
        Self { min_quorum_acks }
    }

    /// Check if write admission is allowed based on current replica ACKs.
    ///
    /// Returns true if alive_acks >= min_quorum_acks.
    pub const fn can_admit_write(&self, alive_acks: usize) -> bool {
        alive_acks >= self.min_quorum_acks
    }

    /// Async mode: no quorum requirement.
    pub const fn async_mode() -> Self {
        Self { min_quorum_acks: 0 }
    }

    /// Build quorum consensus from membership (majority size).
    pub const fn from_membership_majority(membership_size: usize) -> Self {
        Self {
            min_quorum_acks: majority_quorum_size(membership_size),
        }
    }
}
