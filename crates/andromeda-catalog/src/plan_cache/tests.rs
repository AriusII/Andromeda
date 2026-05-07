use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};

use crate::contracts::{PolicyVersion, ProcedureContractBinding, StatsVersion};

use super::*;

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

fn shaped_fingerprint() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish()
}

#[test]
fn plan_class_variant_count_is_bounded() {
    // Asserts the bounded set is exactly what doctrine permits.  Any
    // accidental new variant must update both this constant and the
    // documented taxonomy in the module preamble.
    assert_eq!(PlanClass::VARIANT_COUNT, 4);
    let tags = [
        PlanClass::Singleton.as_tag(),
        PlanClass::ParameterShape.as_tag(),
        PlanClass::Cardinality.as_tag(),
        PlanClass::StatsAdaptive.as_tag(),
    ];
    let mut sorted = tags;
    sorted.sort_unstable();
    assert!(
        sorted.windows(2).all(|pair| pair[0] != pair[1]),
        "PlanClass tags must be unique"
    );
}

#[test]
fn cardinality_bucket_boundaries_are_stable() {
    assert_eq!(CardinalityBucket::VARIANT_COUNT, 6);
    assert_eq!(CardinalityBucket::classify(0), CardinalityBucket::Zero);
    assert_eq!(CardinalityBucket::classify(1), CardinalityBucket::One);
    assert_eq!(CardinalityBucket::classify(2), CardinalityBucket::Small);
    assert_eq!(CardinalityBucket::classify(64), CardinalityBucket::Small);
    assert_eq!(CardinalityBucket::classify(65), CardinalityBucket::Medium);
    assert_eq!(
        CardinalityBucket::classify(4_096),
        CardinalityBucket::Medium
    );
    assert_eq!(CardinalityBucket::classify(4_097), CardinalityBucket::Large);
    assert_eq!(
        CardinalityBucket::classify(262_144),
        CardinalityBucket::Large
    );
    assert_eq!(
        CardinalityBucket::classify(262_145),
        CardinalityBucket::Bulk
    );
}

#[test]
fn plan_shape_fingerprint_is_deterministic() {
    let lhs = shaped_fingerprint();
    let rhs = shaped_fingerprint();
    assert_eq!(lhs, rhs);
    assert!(!lhs.is_empty());
    assert_ne!(lhs, PlanShapeFingerprint::empty());
}

#[test]
fn plan_shape_fingerprint_separates_on_each_input() {
    let base = shaped_fingerprint();

    // Different parameter type tag.
    let other_type = PlanShapeFingerprintBuilder::new()
        .push_parameter(0x09, false, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish();
    assert_ne!(base, other_type);

    // Different nullability.
    let other_null = PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, true, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish();
    assert_ne!(base, other_null);

    // Different cardinality bucket.
    let other_card = PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(10_000))
        .finish();
    assert_ne!(base, other_card);

    // Different parameter ordering (slot order matters).
    let reordered = PlanShapeFingerprintBuilder::new()
        .push_parameter(0x02, true, 0)
        .push_parameter(0x01, false, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish();
    assert_ne!(base, reordered);
}

#[test]
fn plan_cache_key_singleton_rejects_shape_fingerprint() {
    let bind = binding(1, 1, 0xAA, 1, 0xBB);
    let err = PlanCacheKey::build(&bind, PlanClass::Singleton, shaped_fingerprint()).unwrap_err();
    assert_eq!(err, PlanCacheKeyError::SingletonRejectsShapeFingerprint);
}

#[test]
fn plan_cache_key_shaped_class_requires_fingerprint() {
    let bind = binding(1, 1, 0xAA, 1, 0xBB);
    let err = PlanCacheKey::build(
        &bind,
        PlanClass::ParameterShape,
        PlanShapeFingerprint::empty(),
    )
    .unwrap_err();
    assert_eq!(err, PlanCacheKeyError::ShapedPlanClassRequiresFingerprint);
}

#[test]
fn plan_cache_key_rejects_unbound_identity_inputs() {
    let cases = [
        (
            binding(0, 1, 0xAA, 1, 0xBB),
            PlanCacheKeyError::ProcedureIdZero,
        ),
        (
            binding(1, 0, 0xAA, 1, 0xBB),
            PlanCacheKeyError::CatalogVersionZero,
        ),
        (
            binding(1, 1, 0x00, 1, 0xBB),
            PlanCacheKeyError::ContractHashZero,
        ),
        (
            binding(1, 1, 0xAA, 0, 0xBB),
            PlanCacheKeyError::StatsVersionZero,
        ),
        (
            binding(1, 1, 0xAA, 1, 0x00),
            PlanCacheKeyError::PolicyVersionZero,
        ),
    ];

    for (bind, expected) in cases {
        let err = PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty())
            .unwrap_err();
        assert_eq!(err, expected);
    }
}

#[test]
fn plan_cache_key_is_deterministic() {
    let bind = binding(7, 3, 0xAA, 2, 0xBB);
    let lhs = PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty())
        .expect("singleton + empty fingerprint is valid");
    let rhs = PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty())
        .expect("singleton + empty fingerprint is valid");
    assert_eq!(lhs, rhs);
    assert_eq!(lhs.digest(), rhs.digest());
}

#[test]
fn plan_cache_key_separates_on_every_version_field() {
    let base_bind = binding(7, 3, 0xAA, 2, 0xBB);
    let base = PlanCacheKey::build(
        &base_bind,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();

    // Different ContractHash.
    let other = PlanCacheKey::build(
        &binding(7, 3, 0xCC, 2, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    assert_ne!(base, other);
    assert_ne!(base.digest(), other.digest());

    // Different CatalogVersion.
    let other = PlanCacheKey::build(
        &binding(7, 4, 0xAA, 2, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    assert_ne!(base, other);
    assert_ne!(base.digest(), other.digest());

    // Different StatsVersion.
    let other = PlanCacheKey::build(
        &binding(7, 3, 0xAA, 9, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    assert_ne!(base, other);
    assert_ne!(base.digest(), other.digest());

    // Different PolicyVersion.
    let other = PlanCacheKey::build(
        &binding(7, 3, 0xAA, 2, 0xEE),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    assert_ne!(base, other);
    assert_ne!(base.digest(), other.digest());

    // Different ProcedureId.
    let other = PlanCacheKey::build(
        &binding(8, 3, 0xAA, 2, 0xBB),
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .unwrap();
    assert_ne!(base, other);
    assert_ne!(base.digest(), other.digest());
}

#[test]
fn plan_cache_key_separates_on_plan_class_and_shape() {
    let bind = binding(7, 3, 0xAA, 2, 0xBB);
    let fp = shaped_fingerprint();

    let shape_key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp).unwrap();
    let card_key = PlanCacheKey::build(&bind, PlanClass::Cardinality, fp).unwrap();
    let adaptive_key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, fp).unwrap();

    assert_ne!(shape_key, card_key);
    assert_ne!(shape_key, adaptive_key);
    assert_ne!(card_key, adaptive_key);
    assert_ne!(shape_key.digest(), card_key.digest());
    assert_ne!(shape_key.digest(), adaptive_key.digest());

    // A different fingerprint within the same class also separates.
    let other_fp = PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_cardinality(0, CardinalityBucket::Bulk)
        .finish();
    let other_shape_key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, other_fp).unwrap();
    assert_ne!(shape_key, other_shape_key);
    assert_ne!(shape_key.digest(), other_shape_key.digest());
}
