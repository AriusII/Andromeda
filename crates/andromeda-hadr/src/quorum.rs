//! Quorum membership, promotion voting, and the pure decision engine.
//!
//! This module owns:
//! * [`HadrQuorumMembership`] — durable membership snapshot with quorum-size
//!   computation.
//! * [`HadrPromotionVote`] — individual voter attestation.
//! * [`HadrPromotionRequest`] — a candidate's promotion claim plus collected
//!   votes.
//! * [`HadrPromotionRejection`] — categorical rejection reasons.
//! * [`HadrPromotionOutcome`] — approved or rejected decision.
//! * [`HadrAuditRecord`] — audit-friendly record preserving every decision
//!   input.
//! * [`evaluate_promotion`] — the pure, deterministic decision function.
//! * [`promotion_outcome_into_result`] — helpers to propagate outcomes as
//!   typed errors.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

use super::{
    fencing::{HadrFencingContext, HadrFencingToken},
    types::{HadrEpoch, HadrNodeId, HadrNodeState},
};

/// Quorum membership snapshot. Membership is durable (a configuration record),
/// not a runtime view. Quorum size is computed as `floor(N / 2) + 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrQuorumMembership {
    members: Vec<HadrNodeId>,
    quorum_size: usize,
}

impl HadrQuorumMembership {
    /// Build a membership snapshot. Rejects empty or duplicate sets so the
    /// quorum computation is well-defined.
    pub fn new(members: Vec<HadrNodeId>) -> AndromedaResult<Self> {
        if members.is_empty() {
            return Err(hadr_error("HADR quorum membership must not be empty"));
        }
        let mut sorted = members.clone();
        sorted.sort();
        for window in sorted.windows(2) {
            if window[0] == window[1] {
                return Err(hadr_error(
                    "HADR quorum membership contains duplicate node ids",
                ));
            }
        }
        if sorted.iter().any(|m| m.is_zero()) {
            return Err(hadr_error("HADR quorum member id must not be zero"));
        }
        let quorum_size = members.len() / 2 + 1;
        Ok(Self {
            members,
            quorum_size,
        })
    }

    pub fn members(&self) -> &[HadrNodeId] {
        &self.members
    }
    pub fn size(&self) -> usize {
        self.members.len()
    }
    pub fn quorum_size(&self) -> usize {
        self.quorum_size
    }
    pub fn contains(&self, id: HadrNodeId) -> bool {
        self.members.contains(&id)
    }
}

/// One vote cast by a quorum member during a promotion attempt.
///
/// The voter attests, at the moment of voting, to its observed epoch and its
/// own durable safe LSN. The decision engine compares these against the
/// candidate's claimed state to detect staleness, divergence, and
/// epoch-monotonicity violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HadrPromotionVote {
    pub voter: HadrNodeId,
    pub voter_observed_epoch: HadrEpoch,
    pub voter_safe_lsn: Lsn,
    /// If set, the voter has direct evidence that the candidate's history
    /// diverged from the voter's at this LSN.
    pub voter_observed_divergence: Option<Lsn>,
    pub granted: bool,
}

impl HadrPromotionVote {
    pub const fn grant(
        voter: HadrNodeId,
        voter_observed_epoch: HadrEpoch,
        voter_safe_lsn: Lsn,
    ) -> Self {
        Self {
            voter,
            voter_observed_epoch,
            voter_safe_lsn,
            voter_observed_divergence: None,
            granted: true,
        }
    }

    pub const fn deny(
        voter: HadrNodeId,
        voter_observed_epoch: HadrEpoch,
        voter_safe_lsn: Lsn,
    ) -> Self {
        Self {
            voter,
            voter_observed_epoch,
            voter_safe_lsn,
            voter_observed_divergence: None,
            granted: false,
        }
    }

    pub fn with_divergence(mut self, divergence_lsn: Lsn) -> Self {
        self.voter_observed_divergence = Some(divergence_lsn);
        self
    }
}

/// Promotion request. Carries the candidate's durable claim plus the votes
/// it collected and the proposed epoch under which it would assume primacy.
#[derive(Debug, Clone)]
pub struct HadrPromotionRequest {
    pub candidate: HadrNodeState,
    pub proposed_epoch: HadrEpoch,
    pub votes: Vec<HadrPromotionVote>,
}

impl HadrPromotionRequest {
    pub fn new(
        candidate: HadrNodeState,
        proposed_epoch: HadrEpoch,
        votes: Vec<HadrPromotionVote>,
    ) -> Self {
        Self {
            candidate,
            proposed_epoch,
            votes,
        }
    }
}

/// Categorical reasons a promotion can be rejected. Each variant maps to a
/// specific operator action; do not collapse them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HadrPromotionRejection {
    /// Candidate's role is not eligible (e.g. it is already Primary).
    CandidateRoleIneligible,
    /// Candidate is not a member of the quorum membership snapshot.
    CandidateNotMember,
    /// Candidate's claimed safe LSN is below at least one granting voter's
    /// safe LSN — promoting it would lose visible commits.
    StaleCandidateLsn,
    /// At least one voter (or the candidate itself) reports divergence
    /// evidence at an LSN at or below the candidate's safe LSN.
    DivergentCandidate,
    /// Granted votes are below the membership's quorum size.
    InsufficientQuorum,
    /// One or more votes came from non-members or duplicated voters.
    InvalidVoteRoster,
    /// Proposed epoch is not strictly greater than the highest observed
    /// epoch in the cluster's fencing context (or any voter's observed
    /// epoch). Required to keep epochs monotonic.
    EpochNotMonotonic,
    /// An active fencing token already holds an epoch greater than or equal
    /// to the proposed epoch — promoting under the proposed epoch would
    /// create two concurrent primaries.
    SplitBrainActiveToken,
}

impl HadrPromotionRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CandidateRoleIneligible => "hadr promotion candidate role is not eligible",
            Self::CandidateNotMember => "hadr promotion candidate is not a quorum member",
            Self::StaleCandidateLsn => {
                "hadr promotion candidate safe LSN is below a granting voter"
            },
            Self::DivergentCandidate => {
                "hadr promotion candidate has divergence evidence at or below its safe LSN"
            },
            Self::InsufficientQuorum => "hadr promotion granted votes are below quorum size",
            Self::InvalidVoteRoster => {
                "hadr promotion vote roster contains non-members or duplicates"
            },
            Self::EpochNotMonotonic => {
                "hadr promotion proposed epoch is not strictly greater than highest observed epoch"
            },
            Self::SplitBrainActiveToken => {
                "hadr promotion would conflict with an active fencing token (split-brain)"
            },
        }
    }

    pub(super) fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

/// Outcome of evaluating a promotion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HadrPromotionOutcome {
    Approved {
        token: HadrFencingToken,
        committed_safe_lsn: Lsn,
        granted_voters: Vec<HadrNodeId>,
    },
    Rejected(HadrPromotionRejection),
}

/// Audit-friendly record of a promotion decision. Captures every input that
/// drove the outcome so an operator (or replay tooling) can reconstruct the
/// decision deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrAuditRecord {
    pub candidate: HadrNodeId,
    pub candidate_role: super::types::HadrNodeRole,
    pub candidate_safe_lsn: Lsn,
    pub candidate_divergence_lsn: Option<Lsn>,
    pub proposed_epoch: HadrEpoch,
    pub members: Vec<HadrNodeId>,
    pub quorum_size: usize,
    pub granted_votes: usize,
    pub max_voter_safe_lsn: Lsn,
    pub highest_observed_epoch: HadrEpoch,
    pub active_token: Option<HadrFencingToken>,
    pub outcome: HadrPromotionOutcome,
}

/// Pure decision engine. No I/O, no transport, no time. Validates a
/// promotion request against a quorum membership snapshot and a fencing
/// context, returning both the categorical outcome and an audit record.
pub fn evaluate_promotion(
    request: &HadrPromotionRequest,
    membership: &HadrQuorumMembership,
    fencing: &HadrFencingContext,
) -> HadrAuditRecord {
    let candidate = request.candidate;

    // Pre-compute audit summary fields that are independent of outcome.
    let max_voter_safe_lsn = request
        .votes
        .iter()
        .filter(|v| v.granted)
        .map(|v| v.voter_safe_lsn)
        .max()
        .unwrap_or(Lsn::ZERO);
    let granted_votes = request.votes.iter().filter(|v| v.granted).count();

    let outcome = decide_promotion(
        request,
        membership,
        fencing,
        max_voter_safe_lsn,
        granted_votes,
    );

    HadrAuditRecord {
        candidate: candidate.id,
        candidate_role: candidate.role,
        candidate_safe_lsn: candidate.safe_lsn,
        candidate_divergence_lsn: candidate.divergence_lsn,
        proposed_epoch: request.proposed_epoch,
        members: membership.members().to_vec(),
        quorum_size: membership.quorum_size(),
        granted_votes,
        max_voter_safe_lsn,
        highest_observed_epoch: fencing.highest_observed_epoch,
        active_token: fencing.active_token,
        outcome,
    }
}

fn decide_promotion(
    request: &HadrPromotionRequest,
    membership: &HadrQuorumMembership,
    fencing: &HadrFencingContext,
    max_voter_safe_lsn: Lsn,
    granted_votes: usize,
) -> HadrPromotionOutcome {
    let candidate = request.candidate;

    // 1. Candidate role must be promotion-eligible.
    if !candidate.role.is_promotion_eligible_role() {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::CandidateRoleIneligible);
    }

    // 2. Candidate must be a member of the quorum.
    if !membership.contains(candidate.id) {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::CandidateNotMember);
    }

    // 3. Vote roster validity: every voter must be a member, no duplicates.
    let mut seen = Vec::with_capacity(request.votes.len());
    for vote in &request.votes {
        if !membership.contains(vote.voter) {
            return HadrPromotionOutcome::Rejected(HadrPromotionRejection::InvalidVoteRoster);
        }
        if seen.contains(&vote.voter) {
            return HadrPromotionOutcome::Rejected(HadrPromotionRejection::InvalidVoteRoster);
        }
        seen.push(vote.voter);
    }

    // 4. Epoch monotonicity. Proposed epoch must strictly exceed both the
    //    highest cluster-observed epoch and every voter's observed epoch.
    if request.proposed_epoch <= fencing.highest_observed_epoch {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::EpochNotMonotonic);
    }
    for vote in &request.votes {
        if request.proposed_epoch <= vote.voter_observed_epoch {
            return HadrPromotionOutcome::Rejected(HadrPromotionRejection::EpochNotMonotonic);
        }
    }

    // 5. Split-brain: if there's an active token whose epoch is at or above
    //    the proposed epoch, the candidate cannot supersede it.
    if let Some(token) = fencing.active_token
        && token.epoch >= request.proposed_epoch
    {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::SplitBrainActiveToken);
    }

    // 6. Quorum count.
    if granted_votes < membership.quorum_size() {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::InsufficientQuorum);
    }

    // 7. Safe-LSN: candidate must be at least as durable as every granting
    //    voter, otherwise promoting would lose visible commits.
    if candidate.safe_lsn < max_voter_safe_lsn {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::StaleCandidateLsn);
    }

    // 8. Divergence: candidate's own divergence evidence at or below its
    //    safe LSN, or any voter's divergence evidence within the candidate's
    //    safe range, blocks promotion.
    if let Some(div) = candidate.divergence_lsn
        && div <= candidate.safe_lsn
    {
        return HadrPromotionOutcome::Rejected(HadrPromotionRejection::DivergentCandidate);
    }
    for vote in &request.votes {
        if let Some(div) = vote.voter_observed_divergence
            && div <= candidate.safe_lsn
        {
            return HadrPromotionOutcome::Rejected(HadrPromotionRejection::DivergentCandidate);
        }
    }

    let token = HadrFencingToken::new(candidate.id, request.proposed_epoch);
    let granted_voters: Vec<HadrNodeId> = request
        .votes
        .iter()
        .filter(|v| v.granted)
        .map(|v| v.voter)
        .collect();
    HadrPromotionOutcome::Approved {
        token,
        committed_safe_lsn: candidate.safe_lsn,
        granted_voters,
    }
}

/// Surface a promotion outcome as a typed error result. Useful when callers
/// want to propagate a rejection up the stack while still owning the audit
/// record separately.
pub fn promotion_outcome_into_result(
    outcome: &HadrPromotionOutcome,
) -> AndromedaResult<HadrFencingToken> {
    match outcome {
        HadrPromotionOutcome::Approved { token, .. } => Ok(*token),
        HadrPromotionOutcome::Rejected(reason) => Err(reason.into_error()),
    }
}

fn hadr_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
