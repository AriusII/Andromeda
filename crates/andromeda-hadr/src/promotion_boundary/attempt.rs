use crate::{
    Lsn,
    fencing::HadrFencingContext,
    quorum::HadrPromotionVote,
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
