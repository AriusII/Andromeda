//! Comprehensive plan cache invalidation tests.
//!
//! These tests verify that `PlanCacheKey` identity ensures cache invalidation
//! when any of its 7 components change:
//! 1. ProcedureId
//! 2. ContractHash
//! 3. CatalogVersion
//! 4. StatsVersion
//! 5. PolicyVersion
//! 6. PlanClass
//! 7. PlanShapeFingerprint
//!
//! The "no-silent-drop" guarantee is the core invariant: when a component
//! changes, the resulting key MUST NOT equal the old key, MUST have a different
//! digest, and a future runtime cache MUST NOT reuse plans across the boundary.

use andromeda_catalog::{
    CardinalityBucket, PlanCacheKey, PlanCacheKeyError, PlanClass, PlanShapeFingerprint,
    PlanShapeFingerprintBuilder, PolicyVersion, ProcedureContractBinding, StatsVersion,
};
use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};

/// Helper to construct a test binding with all fields customizable.
fn binding(
    procedure: u64,
    catalog: u64,
    contract_byte: u8,
    stats: u64,
    policy_byte: u8,
) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: ProcedureId::new(procedure),
        catalog_version: CatalogVersion::new(catalog),
        contract_hash: ContractHash::new([contract_byte; ContractHash::LEN]),
        stats_version: StatsVersion::new(stats),
        policy_version: PolicyVersion::new([policy_byte; PolicyVersion::LEN]),
    }
}

/// Helper to construct a non-empty shaped fingerprint for testing.
fn shaped_fingerprint() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish()
}

/// Helper to construct a different shaped fingerprint (different parameters).
fn shaped_fingerprint_alt() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x03, false, 2)
        .push_cardinality(0, CardinalityBucket::classify(10_000))
        .finish()
}

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
/// **No-silent-drop check**: Old key ≠ new key; old key must be rejected if
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
/// **No-silent-drop check**: Old key ≠ new key; digest differs.
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
/// **No-silent-drop check**: Old key ≠ new key; digest differs.
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

// TEST 5: Cache key components are all required

/// Verifies that every component of the `PlanCacheKey` identity participates
/// in invalidation by testing each component in isolation.
///
/// **Scenario**: Modify each component one at a time and verify that the
/// resulting key differs from the base key.
///
/// **No-silent-drop check**: Each modified key ≠ base key; digests differ.
#[test]
fn test_cache_key_components_all_required() {
    let base_bind = binding(50, 20, 0xAA, 10, 0xBB);
    let base_fp = shaped_fingerprint();

    let base_key = PlanCacheKey::build(&base_bind, PlanClass::StatsAdaptive, base_fp)
        .expect("base binding with stats adaptive class");
    let base_digest = base_key.digest();

    // Test 1: Different ProcedureId
    {
        let modified_bind = binding(51, 20, 0xAA, 10, 0xBB); // procedure 50 → 51
        let modified_key = PlanCacheKey::build(&modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified procedure id");
        assert_ne!(
            base_key, modified_key,
            "ProcedureId component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "ProcedureId must change digest"
        );
    }

    // Test 2: Different ContractHash
    {
        let modified_bind = binding(50, 20, 0xCC, 10, 0xBB); // contract 0xAA → 0xCC
        let modified_key = PlanCacheKey::build(&modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified contract hash");
        assert_ne!(
            base_key, modified_key,
            "ContractHash component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "ContractHash must change digest"
        );
    }

    // Test 3: Different CatalogVersion
    {
        let modified_bind = binding(50, 21, 0xAA, 10, 0xBB); // catalog 20 → 21
        let modified_key = PlanCacheKey::build(&modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified catalog version");
        assert_ne!(
            base_key, modified_key,
            "CatalogVersion component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "CatalogVersion must change digest"
        );
    }

    // Test 4: Different StatsVersion
    {
        let modified_bind = binding(50, 20, 0xAA, 11, 0xBB); // stats 10 → 11
        let modified_key = PlanCacheKey::build(&modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified stats version");
        assert_ne!(
            base_key, modified_key,
            "StatsVersion component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "StatsVersion must change digest"
        );
    }

    // Test 5: Different PolicyVersion
    {
        let modified_bind = binding(50, 20, 0xAA, 10, 0xEE); // policy 0xBB → 0xEE
        let modified_key = PlanCacheKey::build(&modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified policy version");
        assert_ne!(
            base_key, modified_key,
            "PolicyVersion component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "PolicyVersion must change digest"
        );
    }

    // Test 6: Different PlanClass
    {
        let alt_class = PlanClass::Cardinality; // StatsAdaptive → Cardinality
        let modified_key =
            PlanCacheKey::build(&base_bind, alt_class, base_fp).expect("modified plan class");
        assert_ne!(
            base_key, modified_key,
            "PlanClass component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "PlanClass must change digest"
        );
    }

    // Test 7: Different PlanShapeFingerprint
    {
        let alt_fp = shaped_fingerprint_alt(); // different shape evidence
        let modified_key = PlanCacheKey::build(&base_bind, PlanClass::StatsAdaptive, alt_fp)
            .expect("modified shape fingerprint");
        assert_ne!(
            base_key, modified_key,
            "PlanShapeFingerprint component must participate in invalidation"
        );
        assert_ne!(
            base_digest,
            modified_key.digest(),
            "PlanShapeFingerprint must change digest"
        );
    }
}

// TEST 6: Plan cache rejects mismatched contract before use

/// Verifies that `PlanCacheKey::build` rejects contract mismatches and prevents
/// accidental creation of a key from a misaligned binding.
///
/// **Scenario**: A runtime system might accidentally try to reuse a plan from
/// a different contract. This test verifies that the key-build phase catches
/// such errors.
///
/// **No-silent-drop check**: Mismatched contracts result in either:
/// - Different keys (if both are valid)
/// - Rejection at build time (if either is invalid)
#[test]
fn test_plan_cache_rejects_mismatched_contract_before_use() {
    // Procedure 1 with contract A
    let proc1_contract_a = binding(300, 25, 0x11, 6, 0x22);

    // Procedure 1 with contract B (same procedure, different contract)
    let proc1_contract_b = binding(300, 25, 0x99, 6, 0x22);

    let fp = shaped_fingerprint();

    // Build keys for both scenarios.
    let key_a =
        PlanCacheKey::build(&proc1_contract_a, PlanClass::ParameterShape, fp).expect("contract A");
    let key_b =
        PlanCacheKey::build(&proc1_contract_b, PlanClass::ParameterShape, fp).expect("contract B");

    // Keys MUST differ because the contract hashes differ.
    assert_ne!(
        key_a, key_b,
        "same procedure with different contracts must not share keys"
    );

    // This prevents silent reuse: even if the procedure ID matches, the contract
    // hash ensures the old plan is not retrieved.
    assert_ne!(
        key_a, key_b,
        "no-silent-drop: contract mismatch must result in different keys"
    );

    // Verify the digests also differ.
    assert_ne!(
        key_a.digest(),
        key_b.digest(),
        "contract mismatch must result in different digests"
    );

    // Now verify that unbound contracts are rejected at build time.
    let unbound_contract = binding(300, 0, 0x11, 6, 0x22); // catalog_version = 0 (invalid)

    let err = PlanCacheKey::build(&unbound_contract, PlanClass::ParameterShape, fp);
    assert_err_is_catalog_version_zero(&err);
}

// TEST 7: Plan cache no silent reuse across procedures

/// Verifies that two different procedures never share a cached plan key,
/// even if all other components are identical.
///
/// **Scenario**: Two procedures (A and B) might have the same contract hash
/// by coincidence or design. They must still maintain separate cache entries
/// because their procedure IDs differ.
///
/// **No-silent-drop check**: Different ProcedureIds → different keys, always.
#[test]
fn test_plan_cache_no_silent_reuse_across_procedures() {
    // Procedure 1
    let proc1_binding = binding(400, 30, 0x55, 7, 0x66);

    // Procedure 2 with identical contract, catalog, stats, policy (only procedure ID differs)
    let proc2_binding = binding(401, 30, 0x55, 7, 0x66);

    let fp = shaped_fingerprint();

    let key_proc1 = PlanCacheKey::build(
        &proc1_binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("procedure 1");
    let key_proc2 = PlanCacheKey::build(
        &proc2_binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("procedure 2");

    // Keys MUST differ because the procedure IDs differ.
    assert_ne!(
        key_proc1, key_proc2,
        "different procedures must never share cache keys"
    );

    // Digests MUST differ.
    assert_ne!(
        key_proc1.digest(),
        key_proc2.digest(),
        "different procedures must have different digests"
    );

    // No silent reuse: a runtime cache lookup with proc1's key will never
    // retrieve a plan stored under proc2's key.
    assert_ne!(
        key_proc1, key_proc2,
        "no-silent-drop: different procedures must not share keys"
    );

    // Test with shaped plans too.
    let key_proc1_shaped = PlanCacheKey::build(&proc1_binding, PlanClass::ParameterShape, fp)
        .expect("procedure 1 with shape");
    let key_proc2_shaped = PlanCacheKey::build(&proc2_binding, PlanClass::ParameterShape, fp)
        .expect("procedure 2 with shape");

    assert_ne!(
        key_proc1_shaped, key_proc2_shaped,
        "even shaped plans from different procedures must have different keys"
    );
}

// HELPER ASSERTIONS

fn assert_err_is_catalog_version_zero(err: &Result<PlanCacheKey, PlanCacheKeyError>) {
    match err {
        Err(PlanCacheKeyError::CatalogVersionZero) => {
            // Expected
        }
        other => panic!("expected CatalogVersionZero error, got {:?}", other),
    }
}

// ADDITIONAL ENFORCEMENT TESTS

/// Additional test: Verify that plan class enforcement is strict.
/// PlanClass::Singleton rejects any non-empty fingerprint.
#[test]
fn test_plan_cache_singleton_rejects_shape() {
    let bind = binding(500, 35, 0x77, 8, 0x88);
    let fp = shaped_fingerprint();

    let err = PlanCacheKey::build(&bind, PlanClass::Singleton, fp);
    match err {
        Err(PlanCacheKeyError::SingletonRejectsShapeFingerprint) => {
            // Expected: singleton cannot carry shape evidence
        }
        other => panic!(
            "expected SingletonRejectsShapeFingerprint error, got {:?}",
            other
        ),
    }
}

/// Additional test: Verify that shaped plan classes require a non-empty fingerprint.
#[test]
fn test_plan_cache_shaped_class_requires_fingerprint() {
    let bind = binding(600, 40, 0xAA, 9, 0xBB);

    for plan_class in &[
        PlanClass::ParameterShape,
        PlanClass::Cardinality,
        PlanClass::StatsAdaptive,
    ] {
        let err = PlanCacheKey::build(&bind, *plan_class, PlanShapeFingerprint::empty());
        match err {
            Err(PlanCacheKeyError::ShapedPlanClassRequiresFingerprint) => {
                // Expected: shaped classes must carry non-empty fingerprint
            }
            other => panic!(
                "expected ShapedPlanClassRequiresFingerprint error for {:?}, got {:?}",
                plan_class, other
            ),
        }
    }
}

/// Additional test: Verify that all zero-valued identity inputs are rejected.
#[test]
fn test_plan_cache_rejects_all_zero_identities() {
    let fp = shaped_fingerprint();

    let cases = vec![
        (binding(0, 50, 0xAA, 10, 0xBB), "ProcedureId zero"),
        (binding(700, 0, 0xAA, 10, 0xBB), "CatalogVersion zero"),
        (binding(700, 50, 0x00, 10, 0xBB), "ContractHash zero"),
        (binding(700, 50, 0xAA, 0, 0xBB), "StatsVersion zero"),
        (binding(700, 50, 0xAA, 10, 0x00), "PolicyVersion zero"),
    ];

    for (bind, label) in cases {
        let err = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp);
        assert!(err.is_err(), "{}should be rejected", label);
    }
}
