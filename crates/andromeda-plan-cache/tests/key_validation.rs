mod support;

use andromeda_plan_cache::{PlanCacheKey, PlanCacheKeyError, PlanClass, PlanShapeFingerprint};

use support::{binding, shaped_fingerprint, shaped_fingerprint_alt};

#[test]
fn test_cache_key_components_all_required() {
    let base_bind = binding(50, 20, 0xAA, 10, 0xBB);
    let base_fp = shaped_fingerprint();

    let base_key = PlanCacheKey::build(base_bind, PlanClass::StatsAdaptive, base_fp)
        .expect("base binding with stats adaptive class");
    let base_digest = base_key.digest();

    {
        let modified_bind = binding(51, 20, 0xAA, 10, 0xBB);
        let modified_key = PlanCacheKey::build(modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified procedure id");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }

    {
        let modified_bind = binding(50, 20, 0xCC, 10, 0xBB);
        let modified_key = PlanCacheKey::build(modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified contract hash");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }

    {
        let modified_bind = binding(50, 21, 0xAA, 10, 0xBB);
        let modified_key = PlanCacheKey::build(modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified catalog version");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }

    {
        let modified_bind = binding(50, 20, 0xAA, 11, 0xBB);
        let modified_key = PlanCacheKey::build(modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified stats version");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }

    {
        let modified_bind = binding(50, 20, 0xAA, 10, 0xEE);
        let modified_key = PlanCacheKey::build(modified_bind, PlanClass::StatsAdaptive, base_fp)
            .expect("modified policy version");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }

    {
        let modified_key =
            PlanCacheKey::build(base_bind, PlanClass::Cardinality, base_fp).expect("plan class");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }

    {
        let alt_fp = shaped_fingerprint_alt();
        let modified_key = PlanCacheKey::build(base_bind, PlanClass::StatsAdaptive, alt_fp)
            .expect("shape fingerprint");
        assert_ne!(base_key, modified_key);
        assert_ne!(base_digest, modified_key.digest());
    }
}

#[test]
fn test_plan_cache_rejects_mismatched_contract_before_use() {
    let proc1_contract_a = binding(300, 25, 0x11, 6, 0x22);
    let proc1_contract_b = binding(300, 25, 0x99, 6, 0x22);
    let fp = shaped_fingerprint();

    let key_a =
        PlanCacheKey::build(proc1_contract_a, PlanClass::ParameterShape, fp).expect("contract A");
    let key_b =
        PlanCacheKey::build(proc1_contract_b, PlanClass::ParameterShape, fp).expect("contract B");

    assert_ne!(key_a, key_b);
    assert_ne!(key_a.digest(), key_b.digest());

    let unbound_contract = binding(300, 0, 0x11, 6, 0x22);
    let err = PlanCacheKey::build(unbound_contract, PlanClass::ParameterShape, fp);
    assert_err_is_catalog_version_zero(&err);
}

#[test]
fn test_plan_cache_no_silent_reuse_across_procedures() {
    let proc1_binding = binding(400, 30, 0x55, 7, 0x66);
    let proc2_binding = binding(401, 30, 0x55, 7, 0x66);
    let fp = shaped_fingerprint();

    let key_proc1 = PlanCacheKey::build(
        proc1_binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("procedure 1");
    let key_proc2 = PlanCacheKey::build(
        proc2_binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("procedure 2");

    assert_ne!(key_proc1, key_proc2);
    assert_ne!(key_proc1.digest(), key_proc2.digest());

    let key_proc1_shaped = PlanCacheKey::build(proc1_binding, PlanClass::ParameterShape, fp)
        .expect("procedure 1 with shape");
    let key_proc2_shaped = PlanCacheKey::build(proc2_binding, PlanClass::ParameterShape, fp)
        .expect("procedure 2 with shape");
    assert_ne!(key_proc1_shaped, key_proc2_shaped);
}

#[test]
fn test_plan_cache_singleton_rejects_shape() {
    let bind = binding(500, 35, 0x77, 8, 0x88);
    let fp = shaped_fingerprint();

    let err = PlanCacheKey::build(bind, PlanClass::Singleton, fp);
    match err {
        Err(PlanCacheKeyError::SingletonRejectsShapeFingerprint) => {},
        other => panic!(
            "expected SingletonRejectsShapeFingerprint error, got {:?}",
            other
        ),
    }
}

#[test]
fn test_plan_cache_shaped_class_requires_fingerprint() {
    let bind = binding(600, 40, 0xAA, 9, 0xBB);

    for plan_class in &[
        PlanClass::ParameterShape,
        PlanClass::Cardinality,
        PlanClass::StatsAdaptive,
    ] {
        let err = PlanCacheKey::build(bind, *plan_class, PlanShapeFingerprint::empty());
        match err {
            Err(PlanCacheKeyError::ShapedPlanClassRequiresFingerprint) => {},
            other => panic!(
                "expected ShapedPlanClassRequiresFingerprint error for {:?}, got {:?}",
                plan_class, other
            ),
        }
    }
}

#[test]
fn test_plan_cache_rejects_all_zero_identities() {
    let fp = shaped_fingerprint();
    let cases = [
        binding(0, 50, 0xAA, 10, 0xBB),
        binding(700, 0, 0xAA, 10, 0xBB),
        binding(700, 50, 0x00, 10, 0xBB),
        binding(700, 50, 0xAA, 0, 0xBB),
        binding(700, 50, 0xAA, 10, 0x00),
    ];

    for bind in cases {
        let err = PlanCacheKey::build(bind, PlanClass::ParameterShape, fp);
        assert!(err.is_err(), "zero identity input should be rejected");
    }
}

fn assert_err_is_catalog_version_zero(err: &Result<PlanCacheKey, PlanCacheKeyError>) {
    match err {
        Err(PlanCacheKeyError::CatalogVersionZero) => {},
        other => panic!("expected CatalogVersionZero error, got {:?}", other),
    }
}
