use super::*;

// TEST 1: Cache key stability with no changes

/// Verifies that a `PlanCacheKey` remains stable and equal to itself
/// when created from the same binding, plan class, and fingerprint.
///
/// **No-silent-drop check**: Same key created twice should be equal.
#[test]
fn test_cache_key_stability_with_no_changes() {
    let bind = binding(42, 10, 0xAA, 5, 0xBB);
    let fp = shaped_fingerprint();

    // Create the same key twice.
    let key1 = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp)
        .expect("valid binding + non-singleton class + shaped fingerprint");
    let key2 = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp)
        .expect("valid binding + non-singleton class + shaped fingerprint");

    // Both keys must be equal.
    assert_eq!(key1, key2, "identical inputs must produce identical keys");

    // Both keys must have the same digest.
    assert_eq!(
        key1.digest(),
        key2.digest(),
        "identical keys must have identical digests"
    );

    // A runtime cache lookup with key1 should match a stored entry for key2.
    // (We use key equality to simulate cache lookup.)
    assert_eq!(
        key1, key2,
        "no-silent-drop: cache must recognize both keys as identical"
    );
}

// TEST 2: Cache invalidated on catalog version bump

/// Verifies that advancing `CatalogVersion` creates a different cache key.
///
/// **Scenario**: Same procedure, contract, stats, and policy, but the catalog
/// schema evolves (e.g., a column is added to a shared relation). The new
/// catalog version must not reuse plans from the old version.
///
/// **No-silent-drop check**: Old key != new key; old key must be rejected if
/// a cache lookup uses the new key.
#[test]
fn test_cache_invalidated_on_catalog_version_bump() {
    let bind_v1 = binding(7, 1, 0xCC, 3, 0xDD);
    let bind_v2 = binding(7, 2, 0xCC, 3, 0xDD); // same procedure, contract, stats, policy

    let fp = shaped_fingerprint();

    let key_v1 = PlanCacheKey::build(&bind_v1, PlanClass::Cardinality, fp)
        .expect("v1 binding with cardinality class");
    let key_v2 = PlanCacheKey::build(&bind_v2, PlanClass::Cardinality, fp)
        .expect("v2 binding with cardinality class");

    // Keys must not be equal.
    assert_ne!(
        key_v1, key_v2,
        "catalog version bump must create a different key"
    );

    // Digests must differ.
    assert_ne!(
        key_v1.digest(),
        key_v2.digest(),
        "different catalog versions must have different digests"
    );

    // A runtime cache must not reuse a plan stored under key_v1 when
    // looking up key_v2. We verify this by asserting they are distinct.
    assert_ne!(
        key_v1, key_v2,
        "no-silent-drop: old catalog key must not match new key"
    );
}

// TEST 3: Cache invalidated on contract hash change

/// Verifies that changing the contract hash (e.g., via ALTER PROCEDURE)
/// creates a different cache key.
///
/// **Scenario**: Same procedure and procedure ID, but the procedure's
/// signature, result schema, or compatibility policy is altered. The contract
/// hash changes. Plans from the old contract must not execute against the
/// new contract.
///
/// **No-silent-drop check**: Old key != new key; digest differs.
#[test]
fn test_cache_invalidated_on_contract_hash_change() {
    let bind_old = binding(100, 8, 0x11, 4, 0x22); // contract_byte = 0x11
    let bind_new = binding(100, 8, 0x99, 4, 0x22); // same procedure, catalog, stats, policy
    // but contract_byte = 0x99

    let fp = shaped_fingerprint();

    let key_old = PlanCacheKey::build(&bind_old, PlanClass::StatsAdaptive, fp)
        .expect("old contract binding with adaptive class");
    let key_new = PlanCacheKey::build(&bind_new, PlanClass::StatsAdaptive, fp)
        .expect("new contract binding with adaptive class");

    // Keys must not be equal.
    assert_ne!(
        key_old, key_new,
        "contract hash change must create a different key"
    );

    // Digests must differ.
    assert_ne!(
        key_old.digest(),
        key_new.digest(),
        "different contract hashes must have different digests"
    );

    // No silent reuse: old contract's cached plan cannot be used under the new contract.
    assert_ne!(
        key_old, key_new,
        "no-silent-drop: old contract key must not match new contract key"
    );
}

// TEST 4: Cache invalidated on stats version bump

/// Verifies that advancing `StatsVersion` creates a different cache key.
///
/// **Scenario**: Same procedure and contract, but the statistics histogram
/// is updated (e.g., cardinality estimates change or new histogram data
/// is collected). The stats version bumps. Old cardinality-based or
/// stats-adaptive plans must not execute with new statistics.
///
/// **No-silent-drop check**: Old key != new key; digest differs.
#[test]
fn test_cache_invalidated_on_stats_version_bump() {
    let bind_old_stats = binding(200, 15, 0x77, 1, 0x88); // stats_version = 1
    let bind_new_stats = binding(200, 15, 0x77, 2, 0x88); // stats_version = 2

    let fp = shaped_fingerprint();

    let key_old = PlanCacheKey::build(&bind_old_stats, PlanClass::Cardinality, fp)
        .expect("old stats binding with cardinality class");
    let key_new = PlanCacheKey::build(&bind_new_stats, PlanClass::Cardinality, fp)
        .expect("new stats binding with cardinality class");

    // Keys must not be equal.
    assert_ne!(
        key_old, key_new,
        "stats version bump must create a different key"
    );

    // Digests must differ.
    assert_ne!(
        key_old.digest(),
        key_new.digest(),
        "different stats versions must have different digests"
    );

    // No silent reuse: a plan optimized for old statistics cannot be trusted with new statistics.
    assert_ne!(
        key_old, key_new,
        "no-silent-drop: old stats key must not match new stats key"
    );
}
