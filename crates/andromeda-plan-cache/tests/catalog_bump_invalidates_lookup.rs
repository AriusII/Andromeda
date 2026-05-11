#![forbid(unsafe_code)]

//! Regression test: inserting a plan keyed at CatalogVersion V, then bumping
//! the catalog to V+1, must produce a `CacheMiss` classified as
//! `CatalogVersionMismatch` – not a false `CacheHit` – when the bumped key is
//! looked up.
//!
//! Also asserts the `classify_miss` field-ordering contract: `ContractHash` is
//! evaluated before `CatalogVersion`, so a key that differs in *both* fields
//! returns `ContractHashMismatch`, not `CatalogVersionMismatch`.

mod support;

use andromeda_observability::TraceId;
use andromeda_plan_cache::{
    BoundedPlanCache, PlanCacheKey, PlanCacheMissReason, PlanClass, PlanDecisionOutcome,
    PlanShapeFingerprint,
};

use support::{binding, candidate, select_minimal_plan};

/// Core audit-evidence gap: no prior regression test exercised the full
/// "insert → catalog bump → lookup at old V → Hit; lookup at V+1 →
/// CatalogVersionMismatch" sequence end-to-end.
#[test]
fn catalog_version_bump_makes_prior_lookup_miss_with_catalog_version_mismatch() {
    // ── Step 1: build PlanCacheKey at CatalogVersion(7) ─────────────────────
    // All 7 identity fields are non-zero / non-empty (validated by PlanCacheKey::build).
    let bind_v7 = binding(42, 7, 0xAB, 3, 0xCD);
    let key_v7 = PlanCacheKey::build(bind_v7, PlanClass::Singleton, PlanShapeFingerprint::empty())
        .expect("valid singleton key at catalog_version=7");

    // ── Step 2: insert a synthetic plan into BoundedPlanCache ────────────────
    let candidates = [candidate(10, PlanClass::Singleton, 0, 0x55)];
    let selection = select_minimal_plan(key_v7, &candidates, TraceId::new(20_001))
        .expect("valid plan selection for key_v7");
    let mut cache = BoundedPlanCache::new(4).expect("valid bounded capacity");
    let insert_report = cache
        .insert_selection(&selection, TraceId::new(20_002))
        .expect("insertion at catalog_version=7 must succeed");
    assert_eq!(
        insert_report.insert_trace().outcome(),
        PlanDecisionOutcome::CacheInsert
    );
    assert_eq!(cache.len(), 1);

    // ── Step 3: lookup at V=7 → Hit ──────────────────────────────────────────
    let hit = cache
        .lookup(key_v7, TraceId::new(20_003))
        .expect("non-zero trace id permits lookup evidence");
    assert!(
        hit.is_hit(),
        "lookup at the exact inserted key must be a Hit"
    );
    assert_eq!(hit.trace().outcome(), PlanDecisionOutcome::CacheHit);
    assert_eq!(
        hit.trace().cache_miss_reason(),
        None,
        "a Hit must carry no miss reason"
    );

    // ── Step 4: build same key but catalog_version bumped to V+1=8 ───────────
    // All fields identical except catalog_version: the cache must NOT reuse the
    // V=7 entry for a V=8 lookup.
    let bind_v8 = binding(42, 8, 0xAB, 3, 0xCD); // catalog_version=8, contract_hash unchanged
    let key_v8 = PlanCacheKey::build(bind_v8, PlanClass::Singleton, PlanShapeFingerprint::empty())
        .expect("valid singleton key at catalog_version=8");

    // ── Step 5: lookup at V+1=8 → Miss with CatalogVersionMismatch ───────────
    let miss_catalog = cache
        .lookup(key_v8, TraceId::new(20_004))
        .expect("non-zero trace id permits lookup evidence");
    assert!(
        !miss_catalog.is_hit(),
        "bumped catalog_version must not hit the V=7 entry"
    );
    assert_eq!(
        miss_catalog.trace().outcome(),
        PlanDecisionOutcome::CacheMiss
    );
    assert_eq!(
        miss_catalog.trace().cache_miss_reason(),
        Some(PlanCacheMissReason::CatalogVersionMismatch),
        "a catalog bump with unchanged contract_hash must classify as \
         CatalogVersionMismatch; the cache must reject reuse across catalog epochs"
    );
    let miss_reason_str = miss_catalog.trace().as_decision_trace().reason;
    assert!(
        miss_reason_str.contains("catalog-version-mismatch"),
        "decision trace reason must encode the miss kind; got: {miss_reason_str}"
    );

    // ── Step 6: contract_hash differs AND catalog_version is bumped ───────────
    // classify_miss checks ContractHash *before* CatalogVersion (identity.rs
    // classify_miss loop exits on the first field that does not match). Therefore
    // a key that disagrees on *both* fields must surface ContractHashMismatch,
    // not CatalogVersionMismatch.
    let bind_v8_other_hash = binding(42, 8, 0xEF, 3, 0xCD); // contract_byte=0xEF ≠ 0xAB
    let key_v8_other_hash = PlanCacheKey::build(
        bind_v8_other_hash,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("valid singleton key at v8 with different contract_hash");

    let miss_contract = cache
        .lookup(key_v8_other_hash, TraceId::new(20_005))
        .expect("non-zero trace id permits lookup evidence");
    assert!(
        !miss_contract.is_hit(),
        "different contract_hash must not hit the V=7 entry"
    );
    assert_eq!(
        miss_contract.trace().cache_miss_reason(),
        Some(PlanCacheMissReason::ContractHashMismatch),
        "contract_hash is checked before catalog_version in classify_miss; \
         a mismatched hash must surface ContractHashMismatch even if catalog_version also differs"
    );
    let contract_miss_str = miss_contract.trace().as_decision_trace().reason;
    assert!(
        contract_miss_str.contains("contract-hash-mismatch"),
        "decision trace reason must encode contract-hash-mismatch; got: {contract_miss_str}"
    );
}
