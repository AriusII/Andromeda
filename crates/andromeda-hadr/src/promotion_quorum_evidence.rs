use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

use super::{
    fencing::HadrFencingContext,
    membership_store::HadrMembershipSnapshot,
    promotion_boundary::PromotionAttempt,
    quorum::{
        HadrAuditRecord, HadrPromotionRequest, HadrPromotionVote, HadrQuorumMembership,
        evaluate_promotion,
    },
    types::{HadrEpoch, HadrNodeId, HadrNodeState},
};

pub(super) struct PromotionQuorumEvidence {
    candidate_id: HadrNodeId,
    proposed_epoch: HadrEpoch,
    primary_durable_lsn: Lsn,
    candidate: HadrNodeState,
    membership: HadrQuorumMembership,
}

impl PromotionQuorumEvidence {
    pub(super) fn collect(
        membership_snapshot: &HadrMembershipSnapshot,
        attempt: &PromotionAttempt,
    ) -> AndromedaResult<Self> {
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
        let proposed_epoch = proposed_promotion_epoch(membership_snapshot, attempt)?;
        let candidate = HadrNodeState::new(
            attempt.candidate_id,
            member.role,
            attempt.candidate_observed_epoch,
            attempt.candidate_safe_lsn,
        );

        Ok(Self {
            candidate_id: attempt.candidate_id,
            proposed_epoch,
            primary_durable_lsn: attempt.primary_durable_lsn,
            candidate,
            membership,
        })
    }

    pub(super) const fn candidate_id(&self) -> HadrNodeId {
        self.candidate_id
    }

    pub(super) const fn proposed_epoch(&self) -> HadrEpoch {
        self.proposed_epoch
    }

    pub(super) const fn primary_durable_lsn(&self) -> Lsn {
        self.primary_durable_lsn
    }

    pub(super) fn evaluate(
        &self,
        votes: Vec<HadrPromotionVote>,
        fencing: HadrFencingContext,
    ) -> HadrAuditRecord {
        let request = HadrPromotionRequest::new(self.candidate, self.proposed_epoch, votes);
        evaluate_promotion(&request, &self.membership, &fencing)
    }
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

fn promotion_error(message: &str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
