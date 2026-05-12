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

mod attempt;
mod audit;
mod boundary;
mod planner;
mod report;

pub use super::promotion_decision_helpers::{
    FailoverTrigger, PromotionCandidate, PromotionEligibility, PromotionRequirements,
    is_promotion_eligible, select_best_eligible_candidate,
};
pub use attempt::PromotionAttempt;
pub use audit::{
    HadrPromotionAuditLog, HadrPromotionAuditMarker, HadrPromotionAuditReceipt,
    NoopPromotionAuditLog,
};
pub use boundary::{PromotionBoundary, PromotionCommit};
pub use planner::{PromotionPlan, PromotionPlanner};
pub use report::{PromotionReportQuorumVoteV0, PromotionReportResultV0, PromotionReportV0};

fn promotion_error(message: &str) -> andromeda_error::AndromedaError {
    andromeda_error::AndromedaError::new(andromeda_error::AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests;
