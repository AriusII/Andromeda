use andromeda_error::AndromedaResult;

use crate::{
    Lsn,
    cluster_security::{HadrClusterOperation, HadrClusterSecurityEvidence},
    fencing::HadrFencingToken,
    membership_store::HadrMembershipSnapshot,
    promotion_quorum_evidence::PromotionQuorumEvidence,
    quorum::HadrPromotionOutcome,
    types::{HadrEpoch, HadrNodeId},
};

use super::{PromotionAttempt, audit::HadrPromotionAuditMarker, promotion_error};

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
