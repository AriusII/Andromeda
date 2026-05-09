use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

/// Requirements for a replica to be promotion-eligible.
///
/// This struct is immutable once constructed and contains all state needed to determine
/// if a replica may be promoted. It does NOT trigger promotion; F6 must decide that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionRequirements {
    /// Replica's durable LSN (safe_lsn from F3).
    /// Must be >= primary_durable_lsn to avoid losing visibility.
    pub replica_safe_lsn: Lsn,

    /// Primary's durable LSN (from F1 shipping runtime).
    /// Replica must be caught up to this point.
    pub primary_durable_lsn: Lsn,

    /// Whether replica is a member of the quorum membership snapshot.
    /// Only members can be promoted; non-members cannot hold a fencing token.
    pub is_quorum_member: bool,

    /// Whether replica has a working QUIC connection to the cluster.
    /// Promotion requires reachability to acquire fencing token.
    pub has_working_connection: bool,
}

impl PromotionRequirements {
    /// Construct a new promotion requirements snapshot.
    ///
    /// All parameters are captured immutably; the snapshot reflects the replica's
    /// state at the moment of construction.
    pub const fn new(
        replica_safe_lsn: Lsn,
        primary_durable_lsn: Lsn,
        is_quorum_member: bool,
        has_working_connection: bool,
    ) -> Self {
        Self {
            replica_safe_lsn,
            primary_durable_lsn,
            is_quorum_member,
            has_working_connection,
        }
    }

    /// Validate all requirements for promotion eligibility.
    ///
    /// Returns `Ok(())` if the replica is eligible; `Err` with a specific reason otherwise.
    /// This is a pure function; it does not modify state or perform I/O.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.replica_safe_lsn < self.primary_durable_lsn {
            return Err(promotion_error(
                "replica safe_lsn is behind primary durable_lsn; data loss risk",
            ));
        }

        if !self.is_quorum_member {
            return Err(promotion_error(
                "replica is not a member of quorum membership snapshot",
            ));
        }

        if !self.has_working_connection {
            return Err(promotion_error(
                "replica does not have a working QUIC connection",
            ));
        }

        Ok(())
    }

    /// Check if replica safe LSN is caught up to primary durable LSN.
    ///
    /// Returns true if `replica_safe_lsn >= primary_durable_lsn`.
    pub const fn is_durable_lsn_caught_up(&self) -> bool {
        self.replica_safe_lsn.get() >= self.primary_durable_lsn.get()
    }

    /// Return the LSN gap between primary and replica (if any).
    ///
    /// If replica is ahead or equal, returns 0.
    pub const fn lsn_gap(&self) -> u64 {
        let replica_pos = self.replica_safe_lsn.get();
        let primary_pos = self.primary_durable_lsn.get();
        primary_pos.saturating_sub(replica_pos)
    }
}

/// Failover trigger classification.
///
/// This enum documents the types of events that MAY trigger a failover decision in F6+.
/// Presence of a trigger does NOT initiate promotion; F6 orchestration layer decides that.
///
/// These are documented for audit and operator understanding; no logic evaluates them here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverTrigger {
    /// Primary has become unreachable (connection timeout, DNS failure, etc.).
    /// F6 should initiate leader election and candidate promotion.
    PrimaryUnreachable,

    /// Primary failed its health check (heartbeat timeout from quorum).
    /// Indicates primary may be degraded or crashed.
    PrimaryHealthCheckFailed,

    /// Administrator explicitly requested manual failover.
    /// F6 should respect this request and initiate promotion of eligible candidate.
    ManualFailoverRequested,

    /// Primary's fencing token expired or was revoked.
    /// F6 may interpret as signal to elect new primary.
    FencingTokenExpired,

    /// Data divergence detected at primary or replica.
    /// Blocks promotion until operator intervention resolves the divergence.
    DataDivergenceDetected,

    /// Quorum is unachievable (too many replicas down or unreachable).
    /// Promotion blocked until membership reformed or quorum size reconfigured.
    QuorumLost,
}

impl FailoverTrigger {
    /// Human-readable description of this trigger.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrimaryUnreachable => "primary is unreachable",
            Self::PrimaryHealthCheckFailed => "primary failed health check",
            Self::ManualFailoverRequested => "manual failover requested by operator",
            Self::FencingTokenExpired => "primary fencing token expired",
            Self::DataDivergenceDetected => "data divergence detected",
            Self::QuorumLost => "quorum is unachievable",
        }
    }
}

/// Promotion decision: an eligible replica and the constraints under which it may be promoted.
///
/// This struct packages an eligibility requirement with metadata for F6 orchestration.
/// It represents "Replica X is eligible; F6 may promote it subject to these constraints."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionCandidate {
    /// Replica ID that is eligible for promotion.
    pub replica_id: u64,

    /// Eligibility requirements (LSN, membership, connectivity).
    pub requirements: PromotionRequirements,

    /// Replica's current epoch observation (from F3 quorum state).
    /// F6 must ensure proposed_epoch > this value to maintain monotonicity.
    pub observed_epoch: u64,

    /// Rank among all promotion candidates (0 = best).
    /// Lower rank = higher priority for promotion (better LSN or lower ID tie-break).
    pub rank: usize,
}

impl PromotionCandidate {
    /// Construct a promotion candidate.
    pub const fn new(
        replica_id: u64,
        requirements: PromotionRequirements,
        observed_epoch: u64,
        rank: usize,
    ) -> Self {
        Self {
            replica_id,
            requirements,
            observed_epoch,
            rank,
        }
    }

    /// Validate this candidate is promotion-eligible.
    ///
    /// Calls `requirements.validate()` and propagates any errors.
    pub fn validate(&self) -> AndromedaResult<()> {
        self.requirements.validate()
    }

    /// Check if this candidate is better ranked than another.
    ///
    /// Returns true if `self.rank < other.rank` (lower rank is better).
    pub const fn is_higher_ranked_than(&self, other: &PromotionCandidate) -> bool {
        self.rank < other.rank
    }
}

/// Promotion eligibility check result.
///
/// Returned by eligibility check functions to indicate if a replica MAY be promoted
/// and any relevant metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionEligibility {
    /// Replica is eligible for promotion.
    Eligible(PromotionCandidate),

    /// Replica is not eligible; provides reason.
    Ineligible { replica_id: u64, reason: String },
}

impl PromotionEligibility {
    /// Check if this result indicates eligibility.
    pub const fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible(_))
    }

    /// Get the replica ID from this result.
    pub const fn replica_id(&self) -> u64 {
        match self {
            Self::Eligible(c) => c.replica_id,
            Self::Ineligible { replica_id, .. } => *replica_id,
        }
    }

    /// Extract the candidate if eligible, or error otherwise.
    pub fn into_candidate(self) -> AndromedaResult<PromotionCandidate> {
        match self {
            Self::Eligible(c) => Ok(c),
            Self::Ineligible { replica_id, reason } => Err(promotion_error(&format!(
                "replica {} is not promotion-eligible: {}",
                replica_id, reason
            ))),
        }
    }
}

/// Pure function to check if a replica is promotion-eligible.
///
/// Takes a replica's state snapshot and returns eligibility decision.
/// No side effects; deterministic; queryable without I/O.
///
/// # Arguments
/// - `replica_id`: Identifier of replica to check
/// - `requirements`: Promotion requirements snapshot (LSN, membership, connectivity)
/// - `observed_epoch`: Current epoch this replica has observed
/// - `candidate_rank`: Ranking position among all candidates (0 = best)
///
/// # Returns
/// `PromotionEligibility::Eligible(candidate)` if all requirements pass,
/// or `PromotionEligibility::Ineligible` with reason if any requirement fails.
pub fn is_promotion_eligible(
    replica_id: u64,
    requirements: &PromotionRequirements,
    observed_epoch: u64,
    candidate_rank: usize,
) -> PromotionEligibility {
    match requirements.validate() {
        Ok(()) => {
            let candidate =
                PromotionCandidate::new(replica_id, *requirements, observed_epoch, candidate_rank);
            PromotionEligibility::Eligible(candidate)
        },
        Err(e) => PromotionEligibility::Ineligible {
            replica_id,
            reason: e.message().to_string(),
        },
    }
}

/// Validate all promotion candidates and return the best-ranked eligible one (if any).
///
/// This function ranks candidates and selects the first one that passes eligibility.
/// Used by F6 to determine which replica to promote when multiple candidates exist.
///
/// Returns the best candidate if one is eligible; `Err` if none are eligible.
pub fn select_best_eligible_candidate(
    candidates: &[PromotionCandidate],
) -> AndromedaResult<PromotionCandidate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by_key(|c| c.rank);

    for candidate in sorted {
        if candidate.validate().is_ok() {
            return Ok(candidate);
        }
    }

    Err(promotion_error("no eligible promotion candidates found"))
}

fn promotion_error(message: &str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
