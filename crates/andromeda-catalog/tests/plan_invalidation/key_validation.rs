use super::*;

// TEST 5: Cache key components are all required

/// Verifies that every component of the `PlanCacheKey` identity participates
/// in invalidation by testing each component in isolation.
///
/// **Scenario**: Modify each component one at a time and verify that the
/// resulting key differs from the base key.
///
/// **No-silent-drop check**: Each modified key != base key; digests differ.
#[test]
fn test_cache_key_components_all_required() {
    let base_bind = binding(50, 20, 0xAA, 10, 0xBB);
    let base_fp = shaped_fingerprint();

    let base_key = PlanCacheKey::build(&base_bind, PlanClass::StatsAdaptive, base_fp)
        .expect("base binding with stats adaptive class");
    let base_digest = base_key.digest();

    // Test 1: Different ProcedureId
    {
        let modified_bind = binding(51, 20, 0xAA, 10, 0xBB);
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
        let modified_bind = binding(50, 20, 0xCC, 10, 0xBB);
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
        let modified_bind = binding(50, 21, 0xAA, 10, 0xBB);
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
        let modified_bind = binding(50, 20, 0xAA, 11, 0xBB);
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
        let modified_bind = binding(50, 20, 0xAA, 10, 0xEE);
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
        let alt_class = PlanClass::Cardinality;
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
        let alt_fp = shaped_fingerprint_alt();
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
/// **No-silent-drop check**: Different ProcedureIds -> different keys, always.
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
