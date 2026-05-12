use andromeda_error::AndromedaResult;
use andromeda_observability::TraceId;

use crate::{
    Lsn,
    fencing::HadrFencingToken,
    quorum::HadrPromotionRejection,
    types::{HadrEpoch, HadrNodeId},
};

use super::{audit::HadrPromotionAuditReceipt, promotion_error};

/// Per-voter quorum evidence retained by a V0 promotion report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionReportQuorumVoteV0 {
    pub voter: HadrNodeId,
    pub observed_epoch: HadrEpoch,
    pub safe_lsn: Lsn,
}

impl PromotionReportQuorumVoteV0 {
    pub const fn new(voter: HadrNodeId, observed_epoch: HadrEpoch, safe_lsn: Lsn) -> Self {
        Self {
            voter,
            observed_epoch,
            safe_lsn,
        }
    }
}

/// The categorical result stored in a V0 promotion report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionReportResultV0 {
    Approved,
    Rejected,
}

/// Immutable in-memory representation of a promotion decision report.
///
/// This type is evidence only. Constructing or validating it never publishes a
/// primary claim; visible promotion remains owned by [`PromotionBoundary`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionReportV0 {
    pub candidate: HadrNodeId,
    pub proposed_epoch: HadrEpoch,
    pub quorum_size: usize,
    pub quorum_voters: Vec<PromotionReportQuorumVoteV0>,
    pub fencing_token: HadrFencingToken,
    pub fencing_evidence_hash_sha256: [u8; 32],
    pub safe_lsn: Lsn,
    pub result: PromotionReportResultV0,
    pub rejection: Option<HadrPromotionRejection>,
    pub audit_trace_id: TraceId,
    pub audit_receipt: HadrPromotionAuditReceipt,
}

impl PromotionReportV0 {
    pub fn validate(&self) -> AndromedaResult<()> {
        validate_non_zero_identity(self)?;
        validate_quorum_evidence(self)?;
        validate_fencing_evidence(self)?;
        validate_result_consistency(self)?;
        validate_audit_links(self)?;
        Ok(())
    }
}

fn validate_non_zero_identity(report: &PromotionReportV0) -> AndromedaResult<()> {
    if report.candidate.is_zero() {
        return Err(promotion_error(
            "HADR promotion report requires a non-zero candidate id",
        ));
    }
    if report.proposed_epoch.is_zero() {
        return Err(promotion_error(
            "HADR promotion report requires a non-zero proposed epoch",
        ));
    }
    if report.safe_lsn == Lsn::ZERO {
        return Err(promotion_error(
            "HADR promotion report requires a non-zero safe LSN",
        ));
    }
    Ok(())
}

fn validate_quorum_evidence(report: &PromotionReportV0) -> AndromedaResult<()> {
    if report.quorum_size == 0 {
        return Err(promotion_error(
            "HADR promotion report quorum size must be non-zero",
        ));
    }
    if report.quorum_voters.len() < report.quorum_size {
        return Err(promotion_error(
            "HADR promotion report requires quorum-granted voter evidence",
        ));
    }

    let mut voters = Vec::with_capacity(report.quorum_voters.len());
    let mut max_voter_safe_lsn = Lsn::ZERO;
    for vote in &report.quorum_voters {
        if vote.voter.is_zero() {
            return Err(promotion_error(
                "HADR promotion report quorum voter id must not be zero",
            ));
        }
        if vote.observed_epoch.is_zero() {
            return Err(promotion_error(
                "HADR promotion report quorum voter epoch must not be zero",
            ));
        }
        if report.proposed_epoch <= vote.observed_epoch {
            return Err(promotion_error(
                "HADR promotion report proposed epoch is stale against quorum evidence",
            ));
        }
        if vote.safe_lsn == Lsn::ZERO {
            return Err(promotion_error(
                "HADR promotion report quorum voter safe LSN must be non-zero",
            ));
        }
        if voters.contains(&vote.voter) {
            return Err(promotion_error(
                "HADR promotion report quorum evidence contains duplicate voters",
            ));
        }
        voters.push(vote.voter);
        max_voter_safe_lsn = max_voter_safe_lsn.max(vote.safe_lsn);
    }

    if report.safe_lsn < max_voter_safe_lsn {
        return Err(promotion_error(
            "HADR promotion report safe LSN is stale against quorum voter evidence",
        ));
    }
    Ok(())
}

fn validate_fencing_evidence(report: &PromotionReportV0) -> AndromedaResult<()> {
    if report.fencing_token.primary_id.is_zero() {
        return Err(promotion_error(
            "HADR promotion report requires a non-zero fencing token primary id",
        ));
    }
    if report.fencing_token.epoch.is_zero() {
        return Err(promotion_error(
            "HADR promotion report requires a non-zero fencing token epoch",
        ));
    }
    if report.fencing_token.primary_id != report.candidate {
        return Err(promotion_error(
            "HADR promotion report fencing token must bind the candidate",
        ));
    }
    if report.fencing_token.epoch != report.proposed_epoch {
        return Err(promotion_error(
            "HADR promotion report fencing token must bind the proposed epoch",
        ));
    }
    if report
        .fencing_evidence_hash_sha256
        .iter()
        .all(|byte| *byte == 0)
    {
        return Err(promotion_error(
            "HADR promotion report requires non-zero fencing evidence hash",
        ));
    }
    Ok(())
}

fn validate_result_consistency(report: &PromotionReportV0) -> AndromedaResult<()> {
    match (report.result, report.rejection) {
        (PromotionReportResultV0::Approved, None) => Ok(()),
        (PromotionReportResultV0::Approved, Some(_)) => Err(promotion_error(
            "HADR promotion report approved result must not carry a rejection reason",
        )),
        (PromotionReportResultV0::Rejected, Some(_)) => Ok(()),
        (PromotionReportResultV0::Rejected, None) => Err(promotion_error(
            "HADR promotion report rejected result requires a rejection reason",
        )),
    }
}

fn validate_audit_links(report: &PromotionReportV0) -> AndromedaResult<()> {
    if report.audit_trace_id.is_zero() {
        return Err(promotion_error(
            "HADR promotion report requires a non-zero audit trace id",
        ));
    }
    HadrPromotionAuditReceipt::new(
        report.audit_receipt.audit_lsn,
        report.audit_receipt.marker_digest_sha256,
    )?;
    Ok(())
}
