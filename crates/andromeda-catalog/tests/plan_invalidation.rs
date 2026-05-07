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
//! digest, and the runtime cache MUST NOT reuse plans across the boundary.

use andromeda_catalog::{
    AdvisoryEvidenceStatus, BoundedPlanCache, CardinalityBucket, EvidenceConfidence, EvidenceScore,
    PLAN_SELECTION_MAX_SCENARIO_EVIDENCE, PlanCacheError, PlanCacheKey, PlanCacheKeyError,
    PlanCacheMissReason, PlanCandidate, PlanCandidateId, PlanCandidateRank, PlanClass,
    PlanDecisionOutcome, PlanSelectionError, PlanShapeFingerprint, PlanShapeFingerprintBuilder,
    PolicyVersion, ProcedureContractBinding, ScenarioEvidence, ScenarioId, ScenarioKind,
    ScenarioTarget, StatsVersion, ValidityWindow, classify_advisory_evidence_for_key,
    select_minimal_plan,
};
use andromeda_core::{CatalogVersion, ContractHash, EngineTimestamp, ProcedureId};
use andromeda_observe::{CriticalDecisionKind, TraceId};

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

fn ts(ms: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(ms)
}

fn candidate(id: u64, plan_class: PlanClass, rank: u16, digest_byte: u8) -> PlanCandidate {
    PlanCandidate::new(
        PlanCandidateId::new(id).expect("candidate id must be non-zero"),
        plan_class,
        PlanCandidateRank::from_permille(rank).expect("rank must be bounded"),
        [digest_byte; 32],
    )
    .expect("candidate digest must be non-zero")
}

fn scenario_evidence_for_key(
    key: PlanCacheKey,
    scenario_id: u64,
    score: u16,
    confidence: u16,
) -> ScenarioEvidence {
    ScenarioEvidence::new(
        ScenarioId::new(scenario_id).expect("scenario id must be non-zero"),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(score).expect("score must be bounded"),
        EvidenceConfidence::from_permille(confidence).expect("confidence must be bounded"),
        ValidityWindow::new(ts(100), ts(200)).expect("validity window must be bounded"),
    )
    .expect("scenario evidence target must be valid")
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

#[test]
fn minimal_plan_selection_keeps_scenario_evidence_advisory_only() {
    let bind = binding(800, 60, 0xAA, 12, 0xBB);
    let key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [
        candidate(20, PlanClass::StatsAdaptive, 400, 0x20),
        candidate(10, PlanClass::StatsAdaptive, 100, 0x10),
        candidate(30, PlanClass::Cardinality, 0, 0x30),
    ];
    let evidence = scenario_evidence_for_key(key, 1, 1_000, 1_000);

    assert!(!evidence.is_authoritative());
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &evidence, ts(150)),
        AdvisoryEvidenceStatus::AcceptedAdvisory
    );

    let selection =
        select_minimal_plan(key, &candidates, &[evidence], ts(150), TraceId::new(16_001))
            .expect("bounded candidates and evidence should select");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(10).unwrap(),
        "static rank remains the selector authority"
    );
    assert_eq!(selection.trace().advisory_evidence().supplied_count(), 1);
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 1);
    assert_eq!(selection.trace().outcome(), PlanDecisionOutcome::Selected);

    let trace = selection.trace().as_decision_trace();
    assert_eq!(trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(trace.has_explanation());
    assert!(trace.reason.contains("advisory_only=true"));
    assert!(trace.reason.contains("contract_hash="));
    assert!(
        trace
            .reason
            .contains("version_binding=ContractHash+CatalogVersion+StatsVersion")
    );
    assert!(trace.reason.contains("stats_version=12"));
    assert!(trace.reason.contains("policy_version="));
    assert!(
        trace
            .reason
            .contains("scenario_evidence_statuses=accepted-advisory:1")
    );
}

#[test]
fn minimal_plan_selection_traces_rejected_stale_scenario_evidence_without_changing_selection() {
    let bind = binding(801, 61, 0xAA, 13, 0xBB);
    let key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, shaped_fingerprint())
        .expect("valid parameter-shape key");
    let candidates = [
        candidate(11, PlanClass::ParameterShape, 200, 0x11),
        candidate(12, PlanClass::ParameterShape, 300, 0x12),
    ];
    let mut stale_target_evidence = scenario_evidence_for_key(key, 2, 1_000, 1_000);
    stale_target_evidence = ScenarioEvidence::new(
        stale_target_evidence.scenario_id(),
        stale_target_evidence.kind(),
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: StatsVersion::new(key.stats_version.get() + 1),
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        stale_target_evidence.score(),
        stale_target_evidence.confidence(),
        stale_target_evidence.validity(),
    )
    .expect("stale target is structurally valid evidence");

    assert_eq!(
        classify_advisory_evidence_for_key(&key, &stale_target_evidence, ts(150)),
        AdvisoryEvidenceStatus::StatsVersionMismatch
    );

    let selection = select_minimal_plan(
        key,
        &candidates,
        &[stale_target_evidence],
        ts(150),
        TraceId::new(16_002),
    )
    .expect("stale evidence should be rejected, not fatal");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(11).unwrap()
    );
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 0);
    assert_eq!(selection.trace().advisory_evidence().rejected_count(), 1);
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::StatsVersionMismatch),
        1
    );
    assert!(
        selection
            .trace()
            .as_decision_trace()
            .reason
            .contains("scenario_evidence_rejected=1")
    );
}

#[test]
fn minimal_plan_selection_rejects_stale_catalog_scenario_evidence_without_changing_selection() {
    let bind = binding(802, 62, 0xAA, 14, 0xBB);
    let key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, shaped_fingerprint())
        .expect("valid parameter-shape key");
    let candidates = [
        candidate(21, PlanClass::ParameterShape, 200, 0x21),
        candidate(22, PlanClass::ParameterShape, 300, 0x22),
    ];
    let mut stale_catalog_evidence = scenario_evidence_for_key(key, 3, 1_000, 1_000);
    stale_catalog_evidence = ScenarioEvidence::new(
        stale_catalog_evidence.scenario_id(),
        stale_catalog_evidence.kind(),
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: CatalogVersion::new(key.catalog_version.get() + 1),
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        stale_catalog_evidence.score(),
        stale_catalog_evidence.confidence(),
        stale_catalog_evidence.validity(),
    )
    .expect("stale catalog target is structurally valid evidence");

    assert_eq!(
        classify_advisory_evidence_for_key(&key, &stale_catalog_evidence, ts(150)),
        AdvisoryEvidenceStatus::CatalogVersionMismatch
    );

    let selection = select_minimal_plan(
        key,
        &candidates,
        &[stale_catalog_evidence],
        ts(150),
        TraceId::new(16_013),
    )
    .expect("stale catalog evidence should be rejected, not fatal");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(21).unwrap()
    );
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 0);
    assert_eq!(selection.trace().advisory_evidence().rejected_count(), 1);
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::CatalogVersionMismatch),
        1
    );
    assert!(
        selection
            .trace()
            .as_decision_trace()
            .reason
            .contains("catalog_version=62")
    );
}

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
    let selection = select_minimal_plan(base_key, &candidates, &[], ts(150), TraceId::new(16_003))
        .expect("singleton candidate should select");

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
        &[],
        ts(150),
        TraceId::new(16_007),
    )
    .unwrap();
    let selected_b = select_minimal_plan(
        key_b,
        &[candidate(2, PlanClass::Singleton, 0, 0x52)],
        &[],
        ts(150),
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
fn minimal_plan_selection_rejects_zero_trace_id_before_emitting_trace() {
    let bind = binding(910, 81, 0xAA, 17, 0xBB);
    let key =
        PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty()).unwrap();
    let candidates = [candidate(3, PlanClass::Singleton, 0, 0x61)];

    let error = select_minimal_plan(key, &candidates, &[], ts(150), TraceId::new(0)).unwrap_err();

    assert_eq!(error, PlanSelectionError::TraceIdZero);
}

#[test]
fn bounded_plan_cache_rejects_zero_trace_id_for_trace_producing_operations() {
    let bind = binding(911, 81, 0xAA, 17, 0xBB);
    let key =
        PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty()).unwrap();
    let selected = select_minimal_plan(
        key,
        &[candidate(4, PlanClass::Singleton, 0, 0x62)],
        &[],
        ts(150),
        TraceId::new(16_015),
    )
    .unwrap();

    let mut cache = BoundedPlanCache::new(2).unwrap();
    let insert_error = cache
        .insert_selection(&selected, TraceId::new(0))
        .unwrap_err();
    assert_eq!(insert_error, PlanCacheError::TraceIdZero);
    assert_eq!(
        cache.len(),
        0,
        "zero-trace insert must fail before mutating the cache"
    );

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
fn scenario_evidence_key_mismatches_are_rejected_and_traced_without_selecting() {
    let bind = binding(950, 82, 0xAA, 18, 0xBB);
    let key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [
        candidate(30, PlanClass::StatsAdaptive, 100, 0x30),
        candidate(31, PlanClass::StatsAdaptive, 900, 0x31),
    ];

    let plan_class_mismatch = ScenarioEvidence::new(
        ScenarioId::new(30).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(PlanClass::Cardinality),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(200)).unwrap(),
    )
    .unwrap();
    let missing_contract = ScenarioEvidence::new(
        ScenarioId::new(31).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: None,
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(200)).unwrap(),
    )
    .unwrap();
    let contract_mismatch = ScenarioEvidence::new(
        ScenarioId::new(32).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(ContractHash::new([0xCC; ContractHash::LEN])),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(200)).unwrap(),
    )
    .unwrap();
    let expired = ScenarioEvidence::new(
        ScenarioId::new(33).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(120)).unwrap(),
    )
    .unwrap();
    let not_yet_valid = ScenarioEvidence::new(
        ScenarioId::new(34).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(180), ts(220)).unwrap(),
    )
    .unwrap();

    assert_eq!(
        classify_advisory_evidence_for_key(&key, &plan_class_mismatch, ts(150)),
        AdvisoryEvidenceStatus::PlanClassMismatch
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &missing_contract, ts(150)),
        AdvisoryEvidenceStatus::ContractHashMissing
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &contract_mismatch, ts(150)),
        AdvisoryEvidenceStatus::ContractHashMismatch
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &expired, ts(150)),
        AdvisoryEvidenceStatus::Expired
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &not_yet_valid, ts(150)),
        AdvisoryEvidenceStatus::NotYetValid
    );

    let selection = select_minimal_plan(
        key,
        &candidates,
        &[
            plan_class_mismatch,
            missing_contract,
            contract_mismatch,
            expired,
            not_yet_valid,
        ],
        ts(150),
        TraceId::new(16_013),
    )
    .expect("rejected advisory evidence must not block deterministic selection");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(30).unwrap()
    );
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 0);
    assert_eq!(selection.trace().advisory_evidence().rejected_count(), 5);
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::PlanClassMismatch),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::ContractHashMissing),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::ContractHashMismatch),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::Expired),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::NotYetValid),
        1
    );

    let trace = selection.trace().as_decision_trace();
    assert_eq!(trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(trace.reason.contains("catalog_version=82"));
    assert!(trace.reason.contains("stats_version=18"));
    assert!(trace.reason.contains("plan_class=StatsAdaptive"));
    assert!(trace.reason.contains("scenario_evidence_supplied=5"));
    assert!(trace.reason.contains("scenario_evidence_rejected=5"));
    assert!(trace.reason.contains("plan-class-mismatch:1"));
    assert!(trace.reason.contains("contract-hash-missing:1"));
    assert!(trace.reason.contains("contract-hash-mismatch:1"));
    assert!(trace.reason.contains("expired:1"));
    assert!(trace.reason.contains("not-yet-valid:1"));
    assert!(trace.reason.contains("advisory_only=true"));
}

#[test]
fn scenario_evidence_batch_size_is_bounded_before_plan_selection() {
    let bind = binding(951, 83, 0xAA, 19, 0xBB);
    let key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [candidate(40, PlanClass::StatsAdaptive, 100, 0x40)];

    let mut evidence = Vec::new();
    for index in 0..=PLAN_SELECTION_MAX_SCENARIO_EVIDENCE {
        evidence.push(scenario_evidence_for_key(key, 40 + index as u64, 900, 900));
    }

    let error = select_minimal_plan(key, &candidates, &evidence, ts(150), TraceId::new(16_014))
        .unwrap_err();
    assert_eq!(error, PlanSelectionError::TooManyScenarioEvidence);
}
