//! F4 — Promotion and Failover Eligibility Boundary
//!
//! This module defines which promotion and failover decisions happen in F3 (quorum consensus),
//! which are deferred to F6+ (promotion execution), and which are application-level policy.
//!
//! # Key Design Principles
//!
//! 1. **No Automatic Failover:** Promotion eligibility is computed; execution is explicit.
//! 2. **Queryable Without I/O:** All eligibility checks are pure functions on in-memory state.
//! 3. **Durable Separation:** F3 computes eligibility; F6 decides timing and orchestration.
//! 4. **Split-Brain Prevention:** Quorum + LSN alignment guarantees prevent concurrent primaries.
//!
//! # Scope
//!
//! ## F3 (Quorum Runtime) Responsibilities
//! - Compute replica LSN positions (safe_lsn ≥ primary.durable_lsn)
//! - Track quorum membership (member count, health state)
//! - Verify QUIC connectivity (connection up/down)
//! - Return eligibility decision: "This replica MAY be promoted"
//!
//! ## F4 (This Module) Responsibilities
//! - Define the boundary between F3 eligibility and F6 execution
//! - Document eligibility criteria as immutable, queryable requirements
//! - Provide pure functions to check eligibility without side effects
//! - Reject promotion candidates that violate invariants
//!
//! ## F6+ (Promotion Execution) Responsibilities
//! - Implement orchestration and sequencing
//! - Choose promotion candidate (application policy)
//! - Handle timing and timeout constraints
//! - Execute fencing token acquisition and primary demotion
//! - Emit audit events
//!
//! # Invariants
//!
//! 1. **Durability Alignment:** Replica safe_lsn ≥ primary durable_lsn (from F1 shipping)
//! 2. **Membership Consistency:** Replica is member of quorum (from F3)
//! 3. **Connectivity Requirement:** Replica has working QUIC connection (from D4)
//! 4. **Epoch Monotonicity:** Proposed epoch > highest observed epoch
//! 5. **No Split-Brain:** Active fencing token epoch ≥ proposed epoch blocks promotion
//!
//! # Example Usage
//!
//! ```ignore
//! let eligibility = PromotionRequirements::new(
//!     replica_safe_lsn: Lsn::new(1000),
//!     primary_durable_lsn: Lsn::new(950),
//!     is_quorum_member: true,
//!     has_working_connection: true,
//! );
//!
//! match eligibility.validate() {
//!     Ok(_) => println!("Replica is promotion-eligible"),
//!     Err(e) => println!("Replica cannot be promoted: {}", e),
//! }
//!
//! // This is still not promotion! F6 must decide when and how to execute.
//! ```

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

use super::{
    fencing::{HadrFencingContext, HadrFencingToken},
    membership_store::{HadrMembershipSnapshot, HadrMembershipStore},
    quorum::{
        HadrAuditRecord, HadrPromotionOutcome, HadrPromotionRequest, HadrPromotionVote,
        HadrQuorumMembership, evaluate_promotion, promotion_outcome_into_result,
    },
    types::{HadrEpoch, HadrNodeId, HadrNodeState},
};

/// Requirements for a replica to be promotion-eligible.
///
/// This struct is immutable once constructed and contains all state needed to determine
/// if a replica may be promoted. It does NOT trigger promotion; F6 must decide that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionRequirements {
    /// Replica's durable LSN (safe_lsn from F3).
    /// Must be ≥ primary_durable_lsn to avoid losing visibility.
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
        // Requirement 1: Replica must be caught up on durability.
        if self.replica_safe_lsn < self.primary_durable_lsn {
            return Err(promotion_error(
                "replica safe_lsn is behind primary durable_lsn; data loss risk",
            ));
        }

        // Requirement 2: Replica must be a member of the quorum.
        if !self.is_quorum_member {
            return Err(promotion_error(
                "replica is not a member of quorum membership snapshot",
            ));
        }

        // Requirement 3: Replica must have working connectivity.
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
        }
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

/// Runtime promotion attempt gathered by storage orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionAttempt {
    pub candidate_id: HadrNodeId,
    pub candidate_observed_epoch: HadrEpoch,
    pub candidate_safe_lsn: Lsn,
    pub primary_durable_lsn: Lsn,
    pub votes: Vec<HadrPromotionVote>,
    pub fencing: HadrFencingContext,
}

impl PromotionAttempt {
    pub fn new(
        candidate_id: HadrNodeId,
        candidate_observed_epoch: HadrEpoch,
        candidate_safe_lsn: Lsn,
        primary_durable_lsn: Lsn,
        votes: Vec<HadrPromotionVote>,
        fencing: HadrFencingContext,
    ) -> Self {
        Self {
            candidate_id,
            candidate_observed_epoch,
            candidate_safe_lsn,
            primary_durable_lsn,
            votes,
            fencing,
        }
    }
}

/// Durable audit marker that must be appended before a primary becomes visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrPromotionAuditMarker {
    pub candidate_id: HadrNodeId,
    pub proposed_epoch: HadrEpoch,
    pub primary_durable_lsn: Lsn,
    pub committed_safe_lsn: Lsn,
    pub token: HadrFencingToken,
    pub audit_record: HadrAuditRecord,
}

/// Audit sink used by the storage-side promotion boundary.
pub trait HadrPromotionAuditLog {
    fn append_primary_promotion_marker(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<()>;
}

/// No-op audit sink for callers that only need the membership-store records.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopPromotionAuditLog;

impl HadrPromotionAuditLog for NoopPromotionAuditLog {
    fn append_primary_promotion_marker(
        &self,
        _marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<()> {
        Ok(())
    }
}

/// A fully validated promotion plan. Constructing this does not mutate storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionPlan {
    pub candidate_id: HadrNodeId,
    pub proposed_epoch: HadrEpoch,
    pub committed_safe_lsn: Lsn,
    pub token: HadrFencingToken,
    pub marker: HadrPromotionAuditMarker,
}

/// Pure planner for the storage-side durable promotion runtime.
pub struct PromotionPlanner;

impl PromotionPlanner {
    pub fn plan(
        membership_snapshot: &HadrMembershipSnapshot,
        attempt: PromotionAttempt,
    ) -> AndromedaResult<PromotionPlan> {
        let member = membership_snapshot
            .get(attempt.candidate_id)
            .ok_or_else(|| {
                promotion_error("promotion candidate is not registered in membership store")
            })?;
        if !member.role.is_promotion_eligible_role() {
            return Err(promotion_error(
                "promotion candidate membership role is not promotion eligible",
            ));
        }
        if attempt.candidate_safe_lsn < attempt.primary_durable_lsn {
            return Err(promotion_error(
                "promotion candidate safe LSN is behind primary durable LSN",
            ));
        }

        let membership = quorum_membership_from_snapshot(membership_snapshot)?;
        let proposed_epoch = proposed_promotion_epoch(membership_snapshot, &attempt)?;
        let candidate = HadrNodeState::new(
            attempt.candidate_id,
            member.role,
            attempt.candidate_observed_epoch,
            attempt.candidate_safe_lsn,
        );
        let request = HadrPromotionRequest::new(candidate, proposed_epoch, attempt.votes);
        let audit_record = evaluate_promotion(&request, &membership, &attempt.fencing);
        let token = promotion_outcome_into_result(&audit_record.outcome)?;
        let committed_safe_lsn = match &audit_record.outcome {
            HadrPromotionOutcome::Approved {
                committed_safe_lsn, ..
            } => *committed_safe_lsn,
            HadrPromotionOutcome::Rejected(_) => unreachable!("rejected outcome returned error"),
        };

        let marker = HadrPromotionAuditMarker {
            candidate_id: attempt.candidate_id,
            proposed_epoch,
            primary_durable_lsn: attempt.primary_durable_lsn,
            committed_safe_lsn,
            token,
            audit_record,
        };

        Ok(PromotionPlan {
            candidate_id: attempt.candidate_id,
            proposed_epoch,
            committed_safe_lsn,
            token,
            marker,
        })
    }
}

/// Storage-side durable promotion boundary.
pub struct PromotionBoundary<'a, S, A> {
    membership_store: &'a S,
    audit_log: &'a A,
}

impl<'a, S, A> PromotionBoundary<'a, S, A>
where
    S: HadrMembershipStore,
    A: HadrPromotionAuditLog,
{
    pub const fn new(membership_store: &'a S, audit_log: &'a A) -> Self {
        Self {
            membership_store,
            audit_log,
        }
    }

    pub fn promote(&self, attempt: PromotionAttempt) -> AndromedaResult<PromotionCommit> {
        let membership_snapshot = self
            .membership_store
            .load()?
            .ok_or_else(|| promotion_error("HADR membership snapshot is missing"))?;
        let plan = PromotionPlanner::plan(&membership_snapshot, attempt)?;

        self.audit_log
            .append_primary_promotion_marker(&plan.marker)?;
        let snapshot = self.membership_store.promote_primary(
            plan.candidate_id,
            plan.proposed_epoch,
            plan.committed_safe_lsn,
        )?;

        Ok(PromotionCommit {
            token: plan.token,
            marker: plan.marker,
            snapshot,
        })
    }
}

/// Result of a promotion made visible in the membership store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionCommit {
    pub token: HadrFencingToken,
    pub marker: HadrPromotionAuditMarker,
    pub snapshot: HadrMembershipSnapshot,
}

fn quorum_membership_from_snapshot(
    membership_snapshot: &HadrMembershipSnapshot,
) -> AndromedaResult<HadrQuorumMembership> {
    HadrQuorumMembership::new(
        membership_snapshot
            .nodes()
            .iter()
            .map(|node| node.id)
            .collect(),
    )
}

fn proposed_promotion_epoch(
    membership_snapshot: &HadrMembershipSnapshot,
    attempt: &PromotionAttempt,
) -> AndromedaResult<HadrEpoch> {
    let mut highest = membership_snapshot
        .epoch()
        .max(attempt.fencing.highest_observed_epoch)
        .max(attempt.candidate_observed_epoch);

    for vote in &attempt.votes {
        highest = highest.max(vote.voter_observed_epoch);
    }

    highest
        .checked_next()
        .ok_or_else(|| promotion_error("HADR promotion epoch would overflow"))
}

// Helper function to construct promotion errors
fn promotion_error(message: &str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_requirements(
        replica_lsn: u64,
        primary_lsn: u64,
        is_member: bool,
        has_conn: bool,
    ) -> PromotionRequirements {
        PromotionRequirements::new(
            Lsn::new(replica_lsn),
            Lsn::new(primary_lsn),
            is_member,
            has_conn,
        )
    }

    #[test]
    fn test_requirements_all_valid() {
        let req = make_requirements(1000, 950, true, true);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_requirements_lsn_behind() {
        let req = make_requirements(900, 950, true, true);
        assert!(req.validate().is_err());
        assert_eq!(req.lsn_gap(), 50);
    }

    #[test]
    fn test_requirements_not_member() {
        let req = make_requirements(1000, 950, false, true);
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_requirements_no_connection() {
        let req = make_requirements(1000, 950, true, false);
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_durable_lsn_caught_up() {
        let req1 = make_requirements(1000, 950, true, true);
        assert!(req1.is_durable_lsn_caught_up());

        let req2 = make_requirements(1000, 1000, true, true);
        assert!(req2.is_durable_lsn_caught_up());

        let req3 = make_requirements(900, 950, true, true);
        assert!(!req3.is_durable_lsn_caught_up());
    }

    #[test]
    fn test_lsn_gap_calculation() {
        let req1 = make_requirements(1000, 950, true, true);
        assert_eq!(req1.lsn_gap(), 0);

        let req2 = make_requirements(900, 1000, true, true);
        assert_eq!(req2.lsn_gap(), 100);

        let req3 = make_requirements(1000, 1000, true, true);
        assert_eq!(req3.lsn_gap(), 0);
    }

    #[test]
    fn test_promotion_eligibility_functions() {
        let req_good = make_requirements(1000, 950, true, true);
        let eligibility_good = is_promotion_eligible(1, &req_good, 5, 0);
        assert!(eligibility_good.is_eligible());
        assert_eq!(eligibility_good.replica_id(), 1);

        let req_bad = make_requirements(900, 950, true, true);
        let eligibility_bad = is_promotion_eligible(2, &req_bad, 5, 1);
        assert!(!eligibility_bad.is_eligible());
        assert_eq!(eligibility_bad.replica_id(), 2);
    }

    #[test]
    fn test_promotion_candidate_ranking() {
        let req1 = make_requirements(1000, 950, true, true);
        let req2 = make_requirements(950, 950, true, true);

        let c1 = PromotionCandidate::new(1, req1, 5, 0);
        let c2 = PromotionCandidate::new(2, req2, 5, 1);

        assert!(c1.is_higher_ranked_than(&c2));
        assert!(!c2.is_higher_ranked_than(&c1));
    }

    #[test]
    fn test_select_best_eligible_candidate() -> AndromedaResult<()> {
        let req_good = make_requirements(1000, 950, true, true);
        let req_bad = make_requirements(900, 950, true, true);

        let candidates = vec![
            PromotionCandidate::new(2, req_bad, 5, 0), // Bad, but best rank
            PromotionCandidate::new(1, req_good, 5, 1), // Good, worse rank
        ];

        // Should skip the bad one and select the good one
        let selected = select_best_eligible_candidate(&candidates)?;
        assert_eq!(selected.replica_id, 1);
        Ok(())
    }

    #[test]
    fn test_no_eligible_candidates() {
        let req_bad1 = make_requirements(900, 950, true, true);
        let req_bad2 = make_requirements(800, 950, true, true);

        let candidates = vec![
            PromotionCandidate::new(1, req_bad1, 5, 0),
            PromotionCandidate::new(2, req_bad2, 5, 1),
        ];

        let selected = select_best_eligible_candidate(&candidates);
        assert!(selected.is_err());
    }

    #[test]
    fn test_failover_trigger_descriptions() {
        assert_eq!(
            FailoverTrigger::PrimaryUnreachable.as_str(),
            "primary is unreachable"
        );
        assert_eq!(
            FailoverTrigger::PrimaryHealthCheckFailed.as_str(),
            "primary failed health check"
        );
        assert_eq!(
            FailoverTrigger::ManualFailoverRequested.as_str(),
            "manual failover requested by operator"
        );
    }
}
