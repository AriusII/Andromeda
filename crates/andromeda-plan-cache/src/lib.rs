#![forbid(unsafe_code)]

//! Andromeda plan-cache identity and policy contracts.
//!
//! This crate owns the versioned `PlanCacheKey` identity and the runtime-free
//! policy gate for deciding whether reuse is allowed. It does not store
//! executable plans and does not treat cache output as durable truth.

mod admission;
mod advisory_evidence;
mod decision;
mod error;
mod identity;
mod limits;
mod policy;
mod selection;

pub use admission::{
    BoundedPlanCache, PlanCacheEntry, PlanCacheError, PlanCacheInsertReport, PlanCacheLookupReport,
};
pub use advisory_evidence::{
    AdvisoryEvidenceIdentity, AdvisoryEvidenceStatus, AdvisoryEvidenceSummary,
    AdvisoryEvidenceSummaryBuilder, classify_advisory_identity_for_key,
};
pub use decision::{PlanCacheMissReason, PlanDecisionEvidence, PlanDecisionOutcome};
pub use error::{PlanCacheKeyError, PlanCachePolicyError};
pub use identity::{
    CardinalityBucket, PLAN_CACHE_KEY_SCHEMA_VERSION, PlanCacheKey, PlanCachePublicationIdentity,
    PlanClass, PlanShapeFingerprint, PlanShapeFingerprintBuilder,
};
pub use limits::{
    PLAN_CACHE_MAX_ENTRIES, PLAN_SELECTION_MAX_CANDIDATES, PLAN_SELECTION_MAX_SCENARIO_EVIDENCE,
};
pub use policy::{
    PlanCachePolicy, PlanCacheReuseDecision, PlanCacheReuseReason, evaluate_plan_cache_reuse,
};
pub use selection::{
    PlanCandidate, PlanCandidateId, PlanCandidateRank, PlanSelectionError, PlanSelectionOutcome,
    select_minimal_plan_with_advisory_evidence,
};
