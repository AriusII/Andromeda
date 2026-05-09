use andromeda_error::AndromedaResult;

use crate::membership_store::{HadrMembershipSnapshot, HadrMembershipStore};

use super::{
    PromotionAttempt,
    audit::{HadrPromotionAuditLog, HadrPromotionAuditMarker, HadrPromotionAuditReceipt},
    planner::PromotionPlanner,
    promotion_error,
};

use crate::{cluster_security::HadrClusterSecurityEvidence, fencing::HadrFencingToken};

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
