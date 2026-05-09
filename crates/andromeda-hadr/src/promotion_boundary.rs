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

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

pub use super::promotion_decision_helpers::{
    FailoverTrigger, PromotionCandidate, PromotionEligibility, PromotionRequirements,
    is_promotion_eligible, select_best_eligible_candidate,
};

use super::{
    cluster_security::{HadrClusterOperation, HadrClusterSecurityEvidence},
    fencing::{HadrFencingContext, HadrFencingToken},
    membership_store::{HadrMembershipSnapshot, HadrMembershipStore},
    promotion_quorum_evidence::PromotionQuorumEvidence,
    quorum::{HadrAuditRecord, HadrPromotionOutcome, HadrPromotionVote},
    types::{HadrEpoch, HadrNodeId},
};

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
    pub cluster_security: Option<HadrClusterSecurityEvidence>,
}

/// Durable receipt returned by a promotion audit sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrPromotionAuditReceipt {
    pub audit_lsn: Lsn,
    pub marker_digest_sha256: [u8; 32],
}

impl HadrPromotionAuditReceipt {
    pub fn new(audit_lsn: Lsn, marker_digest_sha256: [u8; 32]) -> AndromedaResult<Self> {
        if audit_lsn == Lsn::ZERO {
            return Err(promotion_error(
                "HADR promotion audit receipt requires a non-zero audit LSN",
            ));
        }
        if marker_digest_sha256.iter().all(|byte| *byte == 0) {
            return Err(promotion_error(
                "HADR promotion audit receipt requires a non-zero marker digest",
            ));
        }
        Ok(Self {
            audit_lsn,
            marker_digest_sha256,
        })
    }
}

/// Audit sink used by the storage-side promotion boundary.
pub trait HadrPromotionAuditLog {
    fn append_primary_promotion_marker(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<()>;

    fn append_primary_promotion_marker_durably(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<HadrPromotionAuditReceipt> {
        let _ = marker;
        Err(promotion_error(
            "HADR promotion audit log did not return a durable audit receipt",
        ))
    }
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
        Self::plan_with_optional_cluster_security(membership_snapshot, attempt, None)
    }

    pub fn plan_with_cluster_security(
        membership_snapshot: &HadrMembershipSnapshot,
        attempt: PromotionAttempt,
        cluster_security: HadrClusterSecurityEvidence,
    ) -> AndromedaResult<PromotionPlan> {
        cluster_security.require_operation(HadrClusterOperation::PromotePrimary)?;
        Self::plan_with_optional_cluster_security(
            membership_snapshot,
            attempt,
            Some(cluster_security),
        )
    }

    fn plan_with_optional_cluster_security(
        membership_snapshot: &HadrMembershipSnapshot,
        attempt: PromotionAttempt,
        cluster_security: Option<HadrClusterSecurityEvidence>,
    ) -> AndromedaResult<PromotionPlan> {
        let evidence = PromotionQuorumEvidence::collect(membership_snapshot, &attempt)?;
        let audit_record = evidence.evaluate(attempt.votes, attempt.fencing);
        let (token, committed_safe_lsn) = match &audit_record.outcome {
            HadrPromotionOutcome::Approved {
                token,
                committed_safe_lsn,
                ..
            } => (*token, *committed_safe_lsn),
            HadrPromotionOutcome::Rejected(reason) => return Err(promotion_error(reason.as_str())),
        };

        let marker = HadrPromotionAuditMarker {
            candidate_id: evidence.candidate_id(),
            proposed_epoch: evidence.proposed_epoch(),
            primary_durable_lsn: evidence.primary_durable_lsn(),
            committed_safe_lsn,
            token,
            audit_record,
            cluster_security,
        };

        Ok(PromotionPlan {
            candidate_id: evidence.candidate_id(),
            proposed_epoch: evidence.proposed_epoch(),
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

    /// Compatibility promotion entry point retained for older
    /// callers. It never publishes a primary because HADR promotion requires
    /// cluster-scope security evidence and a durable audit receipt.
    pub fn promote(&self, _attempt: PromotionAttempt) -> AndromedaResult<PromotionCommit> {
        Err(promotion_error(
            "HADR primary promotion requires cluster security evidence and durable audit receipt; use promote_with_cluster_security",
        ))
    }

    pub fn promote_with_cluster_security(
        &self,
        attempt: PromotionAttempt,
        cluster_security: HadrClusterSecurityEvidence,
    ) -> AndromedaResult<PromotionCommit> {
        let membership_snapshot = self
            .membership_store
            .load()?
            .ok_or_else(|| promotion_error("HADR membership snapshot is missing"))?;
        let plan = PromotionPlanner::plan_with_cluster_security(
            &membership_snapshot,
            attempt,
            cluster_security,
        )?;

        let audit_receipt = self
            .audit_log
            .append_primary_promotion_marker_durably(&plan.marker)?;
        let snapshot = self.membership_store.promote_primary(
            plan.candidate_id,
            plan.proposed_epoch,
            plan.committed_safe_lsn,
        )?;

        Ok(PromotionCommit {
            token: plan.token,
            marker: plan.marker,
            audit_receipt: Some(audit_receipt),
            snapshot,
        })
    }
}

/// Result of a promotion made visible in the membership store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionCommit {
    pub token: HadrFencingToken,
    pub marker: HadrPromotionAuditMarker,
    pub audit_receipt: Option<HadrPromotionAuditReceipt>,
    pub snapshot: HadrMembershipSnapshot,
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
