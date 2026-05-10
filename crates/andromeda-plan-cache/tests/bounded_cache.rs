mod support;

use andromeda_observability::TraceId;
use andromeda_plan_cache::{
    BoundedPlanCache, PlanCacheError, PlanCacheKey, PlanCacheMissReason, PlanClass,
    PlanDecisionOutcome, PlanSelectionError, PlanShapeFingerprint,
};

use support::{binding, candidate, select_minimal_plan, shaped_fingerprint};

#[test]
fn bounded_plan_cache_requires_exact_versioned_key_for_hit() {
    let base_bind = binding(900, 70, 0xAA, 14, 0xBB);
    let base_key = PlanCacheKey::build(
        &base_bind,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("valid singleton key");
    let candidates = [candidate(1, PlanClass::Singleton, 0, 0x41)];
    let selection =
        select_minimal_plan(base_key, &candidates, TraceId::new(16_003)).expect("selection");

    let mut cache = BoundedPlanCache::new(2).expect("bounded non-zero capacity");
    let insert = cache
        .insert_selection(&selection, TraceId::new(16_004))
        .expect("selected plan can be cached");
    assert_eq!(
        insert.insert_trace().outcome(),
        PlanDecisionOutcome::CacheInsert
    );

    let hit = cache
        .lookup(base_key, TraceId::new(16_005))
        .expect("non-zero trace id permits lookup evidence");
    assert!(hit.is_hit(), "same full key should hit");
    assert_eq!(hit.trace().outcome(), PlanDecisionOutcome::CacheHit);
    assert_eq!(hit.trace().cache_miss_reason(), None);
    let hit_reason = hit.trace().as_decision_trace().reason;
    assert!(hit_reason.contains("key_digest="));
    assert!(hit_reason.contains("contract_hash="));
    assert!(hit_reason.contains("catalog_version=70"));
    assert!(hit_reason.contains("stats_version=14"));
    assert!(hit_reason.contains("policy_version="));
    assert!(hit_reason.contains("plan_class=Singleton"));
    assert!(hit_reason.contains("shape_digest="));
    assert!(hit_reason.contains("cache_miss_reason=none"));

    let changed_catalog = PlanCacheKey::build(
        &binding(900, 71, 0xAA, 14, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    let changed_stats = PlanCacheKey::build(
        &binding(900, 70, 0xAA, 15, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    let changed_contract = PlanCacheKey::build(
        &binding(900, 70, 0xCC, 14, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    let changed_policy = PlanCacheKey::build(
        &binding(900, 70, 0xAA, 14, 0xDD),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    let changed_plan_class =
        PlanCacheKey::build(&base_bind, PlanClass::ParameterShape, shaped_fingerprint()).unwrap();

    for (key, expected_reason) in [
        (changed_catalog, PlanCacheMissReason::CatalogVersionMismatch),
        (changed_stats, PlanCacheMissReason::StatsVersionMismatch),
        (changed_contract, PlanCacheMissReason::ContractHashMismatch),
        (changed_policy, PlanCacheMissReason::PolicyVersionMismatch),
        (changed_plan_class, PlanCacheMissReason::PlanClassMismatch),
    ] {
        let miss = cache
            .lookup(key, TraceId::new(16_006))
            .expect("non-zero trace id permits lookup evidence");
        assert!(!miss.is_hit(), "changed key component must miss");
        assert_eq!(miss.trace().outcome(), PlanDecisionOutcome::CacheMiss);
        assert_eq!(miss.trace().cache_miss_reason(), Some(expected_reason));
        let reason = miss.trace().as_decision_trace().reason;
        assert!(reason.contains("cache-miss"));
        assert!(reason.contains(&format!("cache_miss_reason={}", expected_reason.as_str())));
        assert!(reason.contains("policy_version="));
    }
}

#[test]
fn bounded_plan_cache_evicts_oldest_entry_with_trace() {
    let bind_a = binding(901, 80, 0xAA, 16, 0xBB);
    let bind_b = binding(902, 80, 0xAA, 16, 0xBB);
    let key_a =
        PlanCacheKey::build(&bind_a, PlanClass::Singleton, PlanShapeFingerprint::empty()).unwrap();
    let key_b =
        PlanCacheKey::build(&bind_b, PlanClass::Singleton, PlanShapeFingerprint::empty()).unwrap();
    let selected_a = select_minimal_plan(
        key_a,
        &[candidate(1, PlanClass::Singleton, 0, 0x51)],
        TraceId::new(16_007),
    )
    .unwrap();
    let selected_b = select_minimal_plan(
        key_b,
        &[candidate(2, PlanClass::Singleton, 0, 0x52)],
        TraceId::new(16_008),
    )
    .unwrap();

    let mut cache = BoundedPlanCache::new(1).unwrap();
    cache
        .insert_selection(&selected_a, TraceId::new(16_009))
        .unwrap();
    let insert_b = cache
        .insert_selection(&selected_b, TraceId::new(16_010))
        .unwrap();

    assert_eq!(cache.len(), 1);
    assert_eq!(insert_b.evicted().unwrap().key(), key_a);
    assert_eq!(
        insert_b.eviction_trace().unwrap().outcome(),
        PlanDecisionOutcome::CacheEvict
    );
    assert!(
        !cache
            .lookup(key_a, TraceId::new(16_011))
            .expect("non-zero trace id permits miss evidence")
            .is_hit()
    );
    assert!(
        cache
            .lookup(key_b, TraceId::new(16_012))
            .expect("non-zero trace id permits hit evidence")
            .is_hit()
    );
}

#[test]
fn bounded_plan_cache_rejects_zero_trace_id_for_trace_producing_operations() {
    let bind = binding(911, 81, 0xAA, 17, 0xBB);
    let key =
        PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty()).unwrap();
    let selected = select_minimal_plan(
        key,
        &[candidate(4, PlanClass::Singleton, 0, 0x62)],
        TraceId::new(16_015),
    )
    .unwrap();

    let mut cache = BoundedPlanCache::new(2).unwrap();
    let insert_error = cache
        .insert_selection(&selected, TraceId::new(0))
        .unwrap_err();
    assert_eq!(insert_error, PlanCacheError::TraceIdZero);
    assert_eq!(cache.len(), 0);

    cache
        .insert_selection(&selected, TraceId::new(16_016))
        .expect("non-zero trace id permits insert evidence");
    let lookup_error = cache.lookup(key, TraceId::new(0)).unwrap_err();
    assert_eq!(lookup_error, PlanCacheError::TraceIdZero);

    let hit = cache
        .lookup(key, TraceId::new(16_017))
        .expect("non-zero trace id permits lookup evidence");
    assert!(hit.is_hit());
    assert!(!hit.trace().trace_id().is_zero());
}

#[test]
fn minimal_plan_selection_rejects_zero_trace_id_before_emitting_trace() {
    let bind = binding(910, 81, 0xAA, 17, 0xBB);
    let key =
        PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty()).unwrap();
    let candidates = [candidate(3, PlanClass::Singleton, 0, 0x61)];

    let error = select_minimal_plan(key, &candidates, TraceId::new(0)).unwrap_err();
    assert_eq!(error, PlanSelectionError::TraceIdZero);
}
