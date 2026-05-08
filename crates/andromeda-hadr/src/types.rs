//! Primitive identity and state types for HADR participants.
//!
//! Defines the node identifier, epoch counter, role taxonomy, and the durable
//! per-node state snapshot consumed by quorum and fencing decisions.

use crate::Lsn;

/// Stable identifier of a HADR participant. Opaque integer; equality is by
/// raw value. Distinct from WAL shipping node ids because HADR membership is a
/// separate, durable concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct HadrNodeId(u64);

impl HadrNodeId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Monotonic term/epoch. Every successful promotion strictly increments the
/// active epoch. A node observing a token with a higher epoch must yield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct HadrEpoch(u64);

impl HadrEpoch {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
    pub const fn checked_next(self) -> Option<Self> {
        if self.0 == u64::MAX {
            None
        } else {
            Some(Self(self.0 + 1))
        }
    }
}

/// Topology role of a HADR node. Mirrors the WAL shipping role taxonomy but
/// adds an explicit `Candidate` state for the duration of a promotion attempt.
///
/// * `Primary` — currently authoritative under an active fencing token.
/// * `Replica` — read-only follower.
/// * `Candidate` — replica that has staged itself for a promotion attempt at
///   a proposed epoch. A candidate is not yet authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HadrNodeRole {
    Primary,
    Replica,
    Candidate,
}

impl HadrNodeRole {
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::Primary)
    }
    pub const fn is_replica(self) -> bool {
        matches!(self, Self::Replica)
    }
    pub const fn is_candidate(self) -> bool {
        matches!(self, Self::Candidate)
    }
    /// Only replicas and candidates may be promoted. A current primary must
    /// be demoted (or fenced) before re-entering the promotion pipeline.
    pub const fn is_promotion_eligible_role(self) -> bool {
        matches!(self, Self::Replica | Self::Candidate)
    }
}

/// Durable per-node observation used as input to a promotion decision.
///
/// `safe_lsn` is the highest LSN this node has flushed to its WAL (i.e. the
/// LSN it would survive a crash with). `divergence_lsn`, when present, marks
/// an LSN at which this node has observed a chain mismatch — typically while
/// validating a peer's history — and is treated as evidence of an unsafe
/// fork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HadrNodeState {
    pub id: HadrNodeId,
    pub role: HadrNodeRole,
    pub observed_epoch: HadrEpoch,
    pub safe_lsn: Lsn,
    pub divergence_lsn: Option<Lsn>,
}

impl HadrNodeState {
    pub const fn new(
        id: HadrNodeId,
        role: HadrNodeRole,
        observed_epoch: HadrEpoch,
        safe_lsn: Lsn,
    ) -> Self {
        Self {
            id,
            role,
            observed_epoch,
            safe_lsn,
            divergence_lsn: None,
        }
    }

    pub fn with_divergence(mut self, divergence_lsn: Lsn) -> Self {
        self.divergence_lsn = Some(divergence_lsn);
        self
    }
}
