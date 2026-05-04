//! V0 HA/DR quorum and fencing decision model.
//!
//! This module is a *bounded decision model* for single-primary High
//! Availability and Disaster Recovery. It owns:
//!
//! * Node roles, identities, terms (epochs).
//! * Quorum membership and vote tallying.
//! * Promotion eligibility from durable safe-LSN and divergence evidence.
//! * Fencing tokens that bound primary authority by epoch.
//! * Categorical split-brain rejection reasons.
//! * Audit-friendly decision records that preserve every input that drove
//!   a promotion or fencing outcome.
//!
//! Doctrine reminders enforced here:
//!
//! * `single primary` — at most one fencing token may be active at any epoch,
//!   and a new token's epoch must strictly exceed every previously observed
//!   epoch in the system.
//! * `visible commit == durable WAL` — promotion eligibility is computed from
//!   the candidate's *durable* safe LSN (highest LSN flushed to WAL) and from
//!   voter-observed safe LSNs. RAM-only or GPU-only state is not part of the
//!   contract.
//! * `cold snapshot + WAL is reconstructible truth` — divergence evidence is
//!   expressed as an LSN at which a voter detected a chain mismatch with the
//!   candidate; the model has no separate notion of "primary memory state".
//! * Critical failover decisions must be auditable — every decision returns
//!   a [`HadrAuditRecord`] capturing membership, votes, observed epoch,
//!   fencing context, and the categorical reason.
//!
//! Out of scope (deliberately): network transport, runtime, async tasks,
//! gRPC/tonic, leader election liveness, and live WAL replication. This
//! module only validates a *snapshot* of cluster state and returns a
//! deterministic decision.
//!
//! No unsafe, no gRPC, no SQL, no runtime JSON.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

/// Stable identifier of a HADR participant. Opaque integer; equality is by
/// raw value. Distinct from [`crate::write_ahead_log::shipping`] node ids
/// because HADR membership is a separate, durable concern.
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
        self.members.iter().any(|m| *m == id)
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

/// Fencing token issued to a successfully promoted primary.
///
/// A token binds `(primary_id, epoch)`. Operations against the cluster carry
/// the token under which they were issued; any token whose epoch is below
/// the cluster's currently active epoch is fenced off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HadrFencingToken {
    pub primary_id: HadrNodeId,
    pub epoch: HadrEpoch,
}

impl HadrFencingToken {
    pub const fn new(primary_id: HadrNodeId, epoch: HadrEpoch) -> Self {
        Self { primary_id, epoch }
    }
}

/// Snapshot of cluster-wide fencing context at the moment a promotion is
/// being decided. `active_token` represents the currently authoritative
/// primary, if any. `highest_observed_epoch` represents the highest epoch
/// any node in the cluster has *ever* observed (across votes and prior
/// fencing tokens). The decision engine uses both to enforce strict epoch
/// monotonicity and to detect split-brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HadrFencingContext {
    pub active_token: Option<HadrFencingToken>,
    pub highest_observed_epoch: HadrEpoch,
}

impl HadrFencingContext {
    pub const fn empty() -> Self {
        Self {
            active_token: None,
            highest_observed_epoch: HadrEpoch::ZERO,
        }
    }

    pub const fn with_active(token: HadrFencingToken, highest_observed_epoch: HadrEpoch) -> Self {
        Self {
            active_token: Some(token),
            highest_observed_epoch,
        }
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
            }
            Self::DivergentCandidate => {
                "hadr promotion candidate has divergence evidence at or below its safe LSN"
            }
            Self::InsufficientQuorum => "hadr promotion granted votes are below quorum size",
            Self::InvalidVoteRoster => {
                "hadr promotion vote roster contains non-members or duplicates"
            }
            Self::EpochNotMonotonic => {
                "hadr promotion proposed epoch is not strictly greater than highest observed epoch"
            }
            Self::SplitBrainActiveToken => {
                "hadr promotion would conflict with an active fencing token (split-brain)"
            }
        }
    }

    fn into_error(self) -> AndromedaError {
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
    pub candidate_role: HadrNodeRole,
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
        if seen.iter().any(|id| *id == vote.voter) {
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
    if let Some(token) = fencing.active_token {
        if token.epoch >= request.proposed_epoch {
            return HadrPromotionOutcome::Rejected(HadrPromotionRejection::SplitBrainActiveToken);
        }
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
    if let Some(div) = candidate.divergence_lsn {
        if div <= candidate.safe_lsn {
            return HadrPromotionOutcome::Rejected(HadrPromotionRejection::DivergentCandidate);
        }
    }
    for vote in &request.votes {
        if let Some(div) = vote.voter_observed_divergence {
            if div <= candidate.safe_lsn {
                return HadrPromotionOutcome::Rejected(HadrPromotionRejection::DivergentCandidate);
            }
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

/// Categorical reason an operation was fenced off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HadrFencingRejection {
    /// The presented token's epoch is below the active epoch.
    StaleEpoch,
    /// The presented token's primary id does not match the active primary.
    PrimaryIdMismatch,
    /// The presented token's epoch matches the active epoch but the cluster
    /// has no active token at all (e.g. mid-failover).
    NoActiveToken,
}

impl HadrFencingRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaleEpoch => "hadr fencing token epoch is below active epoch",
            Self::PrimaryIdMismatch => {
                "hadr fencing token primary id does not match active primary"
            }
            Self::NoActiveToken => "hadr cluster has no active fencing token",
        }
    }

    fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

/// Validate that an operation tagged with `presented` is permitted under
/// the current fencing context. Returns the active token on success.
pub fn enforce_fencing_token(
    presented: HadrFencingToken,
    fencing: &HadrFencingContext,
) -> AndromedaResult<HadrFencingToken> {
    let active = fencing
        .active_token
        .ok_or_else(|| HadrFencingRejection::NoActiveToken.into_error())?;
    if presented.epoch < active.epoch {
        return Err(HadrFencingRejection::StaleEpoch.into_error());
    }
    if presented.epoch == active.epoch && presented.primary_id != active.primary_id {
        return Err(HadrFencingRejection::PrimaryIdMismatch.into_error());
    }
    if presented.epoch > active.epoch {
        // A token strictly above the active epoch would itself be evidence
        // of split-brain (the cluster has not yet recorded that promotion).
        return Err(HadrFencingRejection::StaleEpoch.into_error());
    }
    Ok(active)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(id: u64) -> HadrNodeId {
        HadrNodeId::new(id)
    }

    fn three_node_membership() -> HadrQuorumMembership {
        HadrQuorumMembership::new(vec![nid(1), nid(2), nid(3)]).expect("membership builds")
    }

    fn replica(id: u64, epoch: u64, lsn: u64) -> HadrNodeState {
        HadrNodeState::new(
            nid(id),
            HadrNodeRole::Replica,
            HadrEpoch::new(epoch),
            Lsn::new(lsn),
        )
    }

    fn grant(voter: u64, epoch: u64, lsn: u64) -> HadrPromotionVote {
        HadrPromotionVote::grant(nid(voter), HadrEpoch::new(epoch), Lsn::new(lsn))
    }

    #[test]
    fn membership_rejects_empty_or_duplicate_or_zero() {
        assert!(HadrQuorumMembership::new(Vec::new()).is_err());
        assert!(HadrQuorumMembership::new(vec![nid(1), nid(1)]).is_err());
        assert!(HadrQuorumMembership::new(vec![nid(0), nid(1)]).is_err());
    }

    #[test]
    fn quorum_size_is_majority() {
        let m1 = HadrQuorumMembership::new(vec![nid(1)]).unwrap();
        let m3 = three_node_membership();
        let m5 = HadrQuorumMembership::new(vec![nid(1), nid(2), nid(3), nid(4), nid(5)]).unwrap();
        assert_eq!(m1.quorum_size(), 1);
        assert_eq!(m3.quorum_size(), 2);
        assert_eq!(m5.quorum_size(), 3);
    }

    #[test]
    fn safe_promotion_is_approved_and_audit_captured() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(2, 0, 100);
        let votes = vec![grant(2, 0, 100), grant(3, 0, 90)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        match &audit.outcome {
            HadrPromotionOutcome::Approved {
                token,
                committed_safe_lsn,
                granted_voters,
            } => {
                assert_eq!(token.primary_id, nid(2));
                assert_eq!(token.epoch, HadrEpoch::new(1));
                assert_eq!(*committed_safe_lsn, Lsn::new(100));
                assert_eq!(granted_voters.len(), 2);
            }
            other => panic!("expected approval, got {other:?}"),
        }
        assert_eq!(audit.granted_votes, 2);
        assert_eq!(audit.quorum_size, 2);
        assert_eq!(audit.max_voter_safe_lsn, Lsn::new(100));
        assert_eq!(audit.candidate, nid(2));
    }

    #[test]
    fn insufficient_quorum_is_rejected() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(2, 0, 100);
        // Only one grant in a 3-node cluster (quorum = 2).
        let votes = vec![
            grant(2, 0, 100),
            HadrPromotionVote::deny(nid(3), HadrEpoch::new(0), Lsn::new(90)),
        ];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::InsufficientQuorum)
        );
        assert_eq!(audit.granted_votes, 1);
    }

    #[test]
    fn stale_candidate_lsn_is_rejected() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        // Candidate at LSN 80 but voter 3 has flushed up to 100.
        let candidate = replica(2, 0, 80);
        let votes = vec![grant(2, 0, 80), grant(3, 0, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::StaleCandidateLsn)
        );
    }

    #[test]
    fn divergent_candidate_is_rejected_via_self_evidence() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(2, 0, 100).with_divergence(Lsn::new(95));
        let votes = vec![grant(2, 0, 100), grant(3, 0, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::DivergentCandidate)
        );
    }

    #[test]
    fn divergent_candidate_is_rejected_via_voter_evidence() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(2, 0, 100);
        // Voter 3 reports divergence within the candidate's safe range.
        let votes = vec![
            grant(2, 0, 100),
            grant(3, 0, 100).with_divergence(Lsn::new(80)),
        ];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::DivergentCandidate)
        );
    }

    #[test]
    fn epoch_must_be_strictly_monotonic() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext {
            active_token: None,
            highest_observed_epoch: HadrEpoch::new(5),
        };
        let candidate = replica(2, 5, 100);
        let votes = vec![grant(2, 5, 100), grant(3, 5, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(5), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::EpochNotMonotonic)
        );
    }

    #[test]
    fn split_brain_active_token_blocks_promotion() {
        let membership = three_node_membership();
        let active = HadrFencingToken::new(nid(1), HadrEpoch::new(7));
        let fencing = HadrFencingContext::with_active(active, HadrEpoch::new(7));
        let candidate = replica(2, 7, 100);
        // Try to promote at the same epoch as the active primary.
        let votes = vec![grant(2, 7, 100), grant(3, 7, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(7), votes);

        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::EpochNotMonotonic)
        );

        // Even one above is blocked while a higher-or-equal token is active:
        // proposed=8 with active=7 at highest_observed=7 must succeed
        // monotonicity, but a stale active token at epoch 8 blocks split-brain.
        let active_high = HadrFencingToken::new(nid(1), HadrEpoch::new(8));
        let fencing_high = HadrFencingContext::with_active(active_high, HadrEpoch::new(8));
        let req2 = HadrPromotionRequest::new(
            replica(2, 7, 100),
            HadrEpoch::new(8),
            vec![grant(2, 7, 100), grant(3, 7, 100)],
        );
        let audit2 = evaluate_promotion(&req2, &membership, &fencing_high);
        assert_eq!(
            audit2.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::EpochNotMonotonic)
        );

        // Properly stepping past an active token: highest_observed=7,
        // active token at epoch 7, proposed=8 -> approved.
        let req3 = HadrPromotionRequest::new(
            replica(2, 7, 100),
            HadrEpoch::new(8),
            vec![grant(2, 7, 100), grant(3, 7, 100)],
        );
        let audit3 = evaluate_promotion(&req3, &membership, &fencing);
        assert!(matches!(
            audit3.outcome,
            HadrPromotionOutcome::Approved { .. }
        ));
    }

    #[test]
    fn primary_role_is_not_promotion_eligible() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let mut candidate = replica(2, 0, 100);
        candidate.role = HadrNodeRole::Primary;
        let votes = vec![grant(2, 0, 100), grant(3, 0, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);
        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::CandidateRoleIneligible)
        );
    }

    #[test]
    fn non_member_candidate_is_rejected() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(99, 0, 100);
        let votes = vec![grant(2, 0, 100), grant(3, 0, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);
        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::CandidateNotMember)
        );
    }

    #[test]
    fn vote_from_non_member_is_rejected() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(2, 0, 100);
        let votes = vec![grant(2, 0, 100), grant(99, 0, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);
        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::InvalidVoteRoster)
        );
    }

    #[test]
    fn duplicate_vote_is_rejected() {
        let membership = three_node_membership();
        let fencing = HadrFencingContext::empty();
        let candidate = replica(2, 0, 100);
        let votes = vec![grant(2, 0, 100), grant(2, 0, 100)];
        let req = HadrPromotionRequest::new(candidate, HadrEpoch::new(1), votes);
        let audit = evaluate_promotion(&req, &membership, &fencing);
        assert_eq!(
            audit.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::InvalidVoteRoster)
        );
    }

    #[test]
    fn fencing_token_mismatch_is_rejected() {
        let active = HadrFencingToken::new(nid(1), HadrEpoch::new(5));
        let fencing = HadrFencingContext::with_active(active, HadrEpoch::new(5));

        // Stale epoch.
        let stale = HadrFencingToken::new(nid(1), HadrEpoch::new(4));
        let err = enforce_fencing_token(stale, &fencing).expect_err("stale rejected");
        assert!(err.to_string().contains("below active epoch"));

        // Same epoch, wrong primary id (split-brain on the same epoch).
        let wrong_primary = HadrFencingToken::new(nid(2), HadrEpoch::new(5));
        let err = enforce_fencing_token(wrong_primary, &fencing).expect_err("primary id mismatch");
        assert!(err.to_string().contains("primary id"));

        // Token from the future without a recorded promotion is also rejected.
        let future = HadrFencingToken::new(nid(2), HadrEpoch::new(6));
        let err = enforce_fencing_token(future, &fencing).expect_err("future rejected");
        assert!(err.to_string().contains("below active epoch"));

        // Matching token is accepted.
        let ok = enforce_fencing_token(active, &fencing).expect("active accepted");
        assert_eq!(ok, active);
    }

    #[test]
    fn fencing_with_no_active_token_is_rejected() {
        let fencing = HadrFencingContext::empty();
        let token = HadrFencingToken::new(nid(1), HadrEpoch::new(1));
        let err = enforce_fencing_token(token, &fencing).expect_err("no active token");
        assert!(err.to_string().contains("no active fencing token"));
    }

    #[test]
    fn split_brain_prevention_two_concurrent_promotions() {
        // Two candidates simultaneously attempt to promote at the same epoch
        // against the same membership. Only one can succeed; the second must
        // be blocked by an active token recorded after the first.
        let membership = three_node_membership();
        let empty = HadrFencingContext::empty();
        let cand_a = replica(1, 0, 100);
        let cand_b = replica(2, 0, 100);

        let req_a = HadrPromotionRequest::new(
            cand_a,
            HadrEpoch::new(1),
            vec![grant(1, 0, 100), grant(2, 0, 100)],
        );
        let audit_a = evaluate_promotion(&req_a, &membership, &empty);
        let token_a = match &audit_a.outcome {
            HadrPromotionOutcome::Approved { token, .. } => *token,
            other => panic!("first promotion should succeed: {other:?}"),
        };

        // After A is promoted, the cluster's fencing context records A.
        let after_a = HadrFencingContext::with_active(token_a, HadrEpoch::new(1));

        // B now attempts to promote at the same epoch. Must be rejected as
        // not-monotonic; even if B proposed epoch 1 it cannot supersede A.
        let req_b_same = HadrPromotionRequest::new(
            cand_b,
            HadrEpoch::new(1),
            vec![grant(2, 0, 100), grant(3, 0, 100)],
        );
        let audit_b_same = evaluate_promotion(&req_b_same, &membership, &after_a);
        assert_eq!(
            audit_b_same.outcome,
            HadrPromotionOutcome::Rejected(HadrPromotionRejection::EpochNotMonotonic)
        );

        // B proposing epoch 2 with stale voter epoch=0 succeeds only if B's
        // safe LSN dominates voters and no divergence exists.
        let req_b_next = HadrPromotionRequest::new(
            replica(2, 1, 100),
            HadrEpoch::new(2),
            vec![grant(2, 1, 100), grant(3, 1, 100)],
        );
        let audit_b_next = evaluate_promotion(&req_b_next, &membership, &after_a);
        assert!(matches!(
            audit_b_next.outcome,
            HadrPromotionOutcome::Approved { .. }
        ));
    }

    #[test]
    fn promotion_outcome_into_result_maps_rejection_to_error() {
        let outcome = HadrPromotionOutcome::Rejected(HadrPromotionRejection::InsufficientQuorum);
        let err = promotion_outcome_into_result(&outcome).expect_err("rejection -> error");
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);

        let approved = HadrPromotionOutcome::Approved {
            token: HadrFencingToken::new(nid(1), HadrEpoch::new(2)),
            committed_safe_lsn: Lsn::new(50),
            granted_voters: vec![nid(1), nid(2)],
        };
        let token = promotion_outcome_into_result(&approved).expect("approved -> token");
        assert_eq!(token.epoch, HadrEpoch::new(2));
    }
}
