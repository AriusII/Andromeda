mod support;

use andromeda_plan_cache::{PlanCacheKey, PlanClass};

use support::{binding, shaped_fingerprint};

#[test]
fn test_cache_key_stability_with_no_changes() {
    let bind = binding(42, 10, 0xAA, 5, 0xBB);
    let fp = shaped_fingerprint();

    let key1 = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp)
        .expect("valid binding + non-singleton class + shaped fingerprint");
    let key2 = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp)
        .expect("valid binding + non-singleton class + shaped fingerprint");

    assert_eq!(key1, key2, "identical inputs must produce identical keys");
    assert_eq!(
        key1.digest(),
        key2.digest(),
        "identical keys must have identical digests"
    );
    assert_eq!(
        key1, key2,
        "no-silent-drop: cache must recognize both keys as identical"
    );
}

#[test]
fn test_cache_invalidated_on_catalog_version_bump() {
    let bind_v1 = binding(7, 1, 0xCC, 3, 0xDD);
    let bind_v2 = binding(7, 2, 0xCC, 3, 0xDD);
    let fp = shaped_fingerprint();

    let key_v1 =
        PlanCacheKey::build(&bind_v1, PlanClass::Cardinality, fp).expect("v1 cardinality key");
    let key_v2 =
        PlanCacheKey::build(&bind_v2, PlanClass::Cardinality, fp).expect("v2 cardinality key");

    assert_ne!(
        key_v1, key_v2,
        "catalog version bump must create a different key"
    );
    assert_ne!(
        key_v1.digest(),
        key_v2.digest(),
        "different catalog versions must have different digests"
    );
    assert_ne!(
        key_v1, key_v2,
        "no-silent-drop: old catalog key must not match new key"
    );
}

#[test]
fn test_cache_invalidated_on_contract_hash_change() {
    let bind_old = binding(100, 8, 0x11, 4, 0x22);
    let bind_new = binding(100, 8, 0x99, 4, 0x22);
    let fp = shaped_fingerprint();

    let key_old =
        PlanCacheKey::build(&bind_old, PlanClass::StatsAdaptive, fp).expect("old contract key");
    let key_new =
        PlanCacheKey::build(&bind_new, PlanClass::StatsAdaptive, fp).expect("new contract key");

    assert_ne!(
        key_old, key_new,
        "contract hash change must create a different key"
    );
    assert_ne!(
        key_old.digest(),
        key_new.digest(),
        "different contract hashes must have different digests"
    );
    assert_ne!(
        key_old, key_new,
        "no-silent-drop: old contract key must not match new contract key"
    );
}

#[test]
fn test_cache_invalidated_on_stats_version_bump() {
    let bind_old_stats = binding(200, 15, 0x77, 1, 0x88);
    let bind_new_stats = binding(200, 15, 0x77, 2, 0x88);
    let fp = shaped_fingerprint();

    let key_old =
        PlanCacheKey::build(&bind_old_stats, PlanClass::Cardinality, fp).expect("old stats key");
    let key_new =
        PlanCacheKey::build(&bind_new_stats, PlanClass::Cardinality, fp).expect("new stats key");

    assert_ne!(
        key_old, key_new,
        "stats version bump must create a different key"
    );
    assert_ne!(
        key_old.digest(),
        key_new.digest(),
        "different stats versions must have different digests"
    );
    assert_ne!(
        key_old, key_new,
        "no-silent-drop: old stats key must not match new stats key"
    );
}
