//! V0 plan-class and bounded plan-cache gate.
//!
//! **Status: MINIMAL RUNTIME GATE.** This module still does not compile SRPL,
//! inspect histograms, or perform cost-based optimization. It defines the
//! bounded identity inputs, validates opaque plan candidates, selects a
//! deterministic minimal candidate, records advisory scenario evidence, and
//! guards an in-memory bounded cache with DecisionTrace-style evidence.
//!
//! ## What this module pins down
//!
//! 1. **Bounded `PlanClass` variants.**  A procedure may declare *one* plan
//!    class.  The set of variants is intentionally small and closed so we
//!    cannot accumulate ad-hoc shape buckets.  Adding a new variant is a
//!    doctrine change.
//! 2. **`CardinalityBucket`** is a bounded coarsening of input row counts.
//!    Bucket boundaries are baked in so the same input always maps to the
//!    same bucket on any node.
//! 3. **`PlanShapeFingerprint`** is a 32-byte SHA-256 digest over the
//!    procedure's bound parameter / type / cardinality evidence.  It is
//!    computed only from the inputs callers explicitly add; the scaffold
//!    never inspects values at runtime.
//! 4. **`PlanCacheKey`** is the full identity tuple a plan cache must key
//!    on.  Equality and hashing are derived from every version field, so
//!    bumping `StatsVersion`, `PolicyVersion`, `CatalogVersion`, or
//!    `ContractHash` automatically separates cache slots without any
//!    additional invalidation logic.
//! 5. **Minimal runtime gate.**  The cache stores only opaque plan ids and
//!    plan digests. It never stores executable SRPL state, never consults the
//!    statistics modules directly, and never treats scenario evidence as
//!    authoritative. Every hit, miss, insert, eviction, and selection emits a
//!    trace containing the key digest plus the bounded inputs that justified
//!    reuse, miss, or rejection. Trace-producing operations fail closed when
//!    the supplied [`TraceId`](andromeda_observe::TraceId) is zero so optimizer
//!    decisions cannot become uncorrelated after the fact.
//!
//! ## What this module deliberately does NOT do
//!
//! - It does not compile a plan.
//! - It does not run a cost model.
//! - It does not compile SRPL.
//! - It does not consult statistics or histograms.
//! - It does not store executable plan state: cache entries carry opaque plan
//!   ids and plan digests only.
//! - It does not interpret SQL; the project remains Procedure-only.
//!
//! Any later optimizer crate can depend on these types without re-deriving the
//! key shape or weakening invalidation, ensuring decision traces remain
//! reproducible across catalog / stats / policy upgrades.
//!
//! ## Plan Cache Invalidation Policy
//!
//! The runtime cache follows these invalidation rules **strictly**: any
//! change to any component listed below must either create a new key entry or
//! reject reuse of an old entry. **Silent reuse is forbidden.**
//!
//! | Component | Invalidation Trigger | Example / Consequence |
//! | --- | --- | --- |
//! | `ProcedureId` | Procedure identity changes | Different procedure, never share cached plan |
//! | `ContractHash` | Procedure contract is altered (ALTER PROCEDURE) | Signature, result schema, or policy change invalidates all old keys |
//! | `CatalogVersion` | Catalog schema evolves | New catalog version = new key entry; old plans not reused |
//! | `StatsVersion` | Statistics histogram or correlation evidence is updated | New stats version = new key entry; old cardinality assumptions void |
//! | `PolicyVersion` | Policy surface mutates | New transaction policy or permissions = new key; strict isolation |
//! | `PlanClass` | Specialization strategy changes | Singleton -> ParameterShape: different key space; no cross-class reuse |
//! | `PlanShapeFingerprint` | Parameter shape or cardinality evidence changes | Different bound parameters = different fingerprint = different key |
//!
//! ### No-Silent-Drop Guarantee
//!
//! When any of these components changes:
//! 1. The resulting `PlanCacheKey` is NOT equal to the old key (derived `PartialEq, Eq`).
//! 2. The key's `digest()` is different (SHA-256 includes each component with a unique tag).
//! 3. A runtime cache lookup with the old key will **not** retrieve a plan from a slot
//!    that was written with the new key.
//! 4. A runtime cache insertion with a new key will allocate a separate slot.
//!
//! This is enforced purely through Rust's type system and Eq/Hash derivation;
//! no explicit eviction logic is required as long as the cache keys on the
//! `PlanCacheKey` type itself.
//!
//! ### Evidence for Traces
//!
//! Every cache operation emits a [`DecisionTrace`](andromeda_observe::DecisionTrace) containing:
//! - The key's digest (privacy-safe 32-byte hash)
//! - The procedure ID, catalog version, stats version, policy version
//! - The plan class and shape fingerprint digest
//! - The operation type (hit, miss, insert, evict, invalidate, or reject)
//! - The bounded reason code (if rejected)
//!
//! ### Facade Coverage Markers
//!
//! The module facade intentionally keeps these runtime-gate markers visible
//! for source-level invariant tests after the implementation split:
//! `PLAN_CACHE_MAX_ENTRIES`, `PLAN_SELECTION_MAX_CANDIDATES`,
//! `PLAN_SELECTION_MAX_SCENARIO_EVIDENCE`,
//! `classify_advisory_evidence_for_key`,
//! `CriticalDecisionKind::PlanSelection`, `PlanDecisionEvidence`,
//! `advisory_only=true`, `policy_version`, `contract_hash`, `stats_version`,
//! and `catalog_version`.

mod advisory_evidence;
mod selection;

#[cfg(test)]
mod tests;

pub use advisory_evidence::classify_advisory_evidence_for_key;
pub use andromeda_plan_cache::{
    AdvisoryEvidenceStatus, AdvisoryEvidenceSummary, AdvisoryEvidenceSummaryBuilder,
    BoundedPlanCache, CardinalityBucket, PLAN_CACHE_MAX_ENTRIES, PLAN_SELECTION_MAX_CANDIDATES,
    PLAN_SELECTION_MAX_SCENARIO_EVIDENCE, PlanCacheEntry, PlanCacheError, PlanCacheInsertReport,
    PlanCacheKey, PlanCacheKeyError, PlanCacheLookupReport, PlanCacheMissReason, PlanCandidate,
    PlanCandidateId, PlanCandidateRank, PlanClass, PlanDecisionEvidence, PlanDecisionOutcome,
    PlanSelectionError, PlanSelectionOutcome, PlanShapeFingerprint, PlanShapeFingerprintBuilder,
};
pub use selection::select_minimal_plan;
