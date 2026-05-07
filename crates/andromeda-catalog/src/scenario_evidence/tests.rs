use andromeda_time::EngineTimestamp;
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::contracts::StatsVersion;
use crate::plan_cache::PlanClass;

use super::*;

fn ts(ms: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(ms)
}

fn target() -> ScenarioTarget {
    ScenarioTarget {
        procedure_id: ProcedureId::new(7),
        catalog_version: CatalogVersion::new(2),
        stats_version: StatsVersion::new(3),
        plan_class: Some(PlanClass::ParameterShape),
        contract_hash: Some(ContractHash::new([0xAB; ContractHash::LEN])),
    }
}

fn evidence_with(
    scenario_id: u64,
    kind: ScenarioKind,
    target: ScenarioTarget,
    score: u16,
    confidence: u16,
    issued: u64,
    expires: u64,
) -> Result<ScenarioEvidence, ScenarioEvidenceError> {
    ScenarioEvidence::new(
        ScenarioId::new(scenario_id).expect("non-zero"),
        kind,
        target,
        EvidenceScore::from_permille(score)?,
        EvidenceConfidence::from_permille(confidence)?,
        ValidityWindow::new(ts(issued), ts(expires))?,
    )
}

fn evidence_at(
    score: u16,
    confidence: u16,
    issued: u64,
    expires: u64,
) -> Result<ScenarioEvidence, ScenarioEvidenceError> {
    evidence_with(
        42,
        ScenarioKind::Microbenchmark,
        target(),
        score,
        confidence,
        issued,
        expires,
    )
}

#[test]
fn scenario_kind_variant_count_is_bounded() {
    assert_eq!(ScenarioKind::VARIANT_COUNT, 4);
    let tags = [
        ScenarioKind::Microbenchmark.as_tag(),
        ScenarioKind::ProcedureWorkload.as_tag(),
        ScenarioKind::RegressionProbe.as_tag(),
        ScenarioKind::SyntheticLoad.as_tag(),
    ];
    let mut sorted = tags;
    sorted.sort_unstable();
    assert!(
        sorted.windows(2).all(|pair| pair[0] != pair[1]),
        "ScenarioKind tags must be unique"
    );
}

#[test]
fn scenario_id_rejects_zero() {
    assert!(ScenarioId::new(0).is_none());
    assert_eq!(ScenarioId::new(5).unwrap().get(), 5);
}

#[test]
fn score_and_confidence_enforce_bounds() {
    assert!(EvidenceScore::from_permille(0).is_ok());
    assert!(EvidenceScore::from_permille(1_000).is_ok());
    assert_eq!(
        EvidenceScore::from_permille(1_001),
        Err(ScenarioEvidenceError::ScoreOutOfRange)
    );

    assert!(EvidenceConfidence::from_permille(0).is_ok());
    assert!(EvidenceConfidence::from_permille(1_000).is_ok());
    assert_eq!(
        EvidenceConfidence::from_permille(1_001),
        Err(ScenarioEvidenceError::ConfidenceOutOfRange)
    );
}

#[test]
fn validity_window_requires_issued_before_expiry_and_nonzero_expiry() {
    assert_eq!(
        ValidityWindow::new(ts(10), ts(0)),
        Err(ScenarioEvidenceError::ExpiryMustBeNonZero)
    );
    assert_eq!(
        ValidityWindow::new(ts(10), ts(10)),
        Err(ScenarioEvidenceError::IssuedNotBeforeExpiry)
    );
    assert_eq!(
        ValidityWindow::new(ts(20), ts(10)),
        Err(ScenarioEvidenceError::IssuedNotBeforeExpiry)
    );
    assert!(ValidityWindow::new(ts(10), ts(20)).is_ok());
}

#[test]
fn target_validation_rejects_zero_identity_fields() {
    let mut t = target();
    t.procedure_id = ProcedureId::new(0);
    assert_eq!(
        evidence_with(1, ScenarioKind::Microbenchmark, t, 0, 0, 1, 2,),
        Err(ScenarioEvidenceError::TargetProcedureIdZero)
    );

    let mut t = target();
    t.catalog_version = CatalogVersion::new(0);
    assert_eq!(
        evidence_with(1, ScenarioKind::Microbenchmark, t, 0, 0, 1, 2,),
        Err(ScenarioEvidenceError::TargetCatalogVersionZero)
    );

    let mut t = target();
    t.stats_version = StatsVersion::new(0);
    assert_eq!(
        evidence_with(1, ScenarioKind::Microbenchmark, t, 0, 0, 1, 2,),
        Err(ScenarioEvidenceError::TargetStatsVersionZero)
    );

    let mut t = target();
    t.contract_hash = Some(ContractHash::new([0u8; ContractHash::LEN]));
    assert_eq!(
        evidence_with(1, ScenarioKind::Microbenchmark, t, 0, 0, 1, 2,),
        Err(ScenarioEvidenceError::TargetContractHashZero)
    );
}

#[test]
fn evidence_is_always_advisory() {
    // Doctrine: even at peak score and confidence, the record is never
    // authoritative. This is a type-level invariant, not a runtime flag the
    // caller could flip.
    let ev = evidence_at(1_000, 1_000, 100, 200).unwrap();
    assert!(!ev.is_authoritative());
    assert_eq!(
        ev.optimizer_boundary(),
        ScenarioEvidenceOptimizerBoundary::AdvisoryOnly
    );
    assert!(!ev.can_select_plan_alone());
    assert!(!ev.can_drive_active_stats_version_transition());
}

#[test]
fn validate_for_use_at_rejects_expired_and_not_yet_valid() {
    let ev = evidence_at(500, 800, 100, 200).unwrap();

    // Before issuance.
    assert_eq!(
        ev.validate_for_use_at(ts(50)),
        Err(ScenarioEvidenceError::NotYetValid)
    );
    // At issuance: valid.
    assert!(ev.validate_for_use_at(ts(100)).is_ok());
    // Inside window.
    assert!(ev.validate_for_use_at(ts(150)).is_ok());
    // At expiry: expired (half-open).
    assert_eq!(
        ev.validate_for_use_at(ts(200)),
        Err(ScenarioEvidenceError::Expired)
    );
    // After expiry.
    assert_eq!(
        ev.validate_for_use_at(ts(999)),
        Err(ScenarioEvidenceError::Expired)
    );

    assert!(!ev.is_expired_at(ts(199)));
    assert!(ev.is_expired_at(ts(200)));
}

#[test]
fn advisory_use_token_requires_valid_window_and_stays_non_authoritative() {
    let ev = evidence_at(1_000, 1_000, 100, 200).unwrap();

    assert_eq!(
        ev.advisory_use_at(ts(50)),
        Err(ScenarioEvidenceError::NotYetValid)
    );
    assert_eq!(
        ev.advisory_use_at(ts(200)),
        Err(ScenarioEvidenceError::Expired)
    );

    let advisory = ev.advisory_use_at(ts(150)).unwrap();
    assert_eq!(advisory.scenario_id(), ev.scenario_id());
    assert_eq!(advisory.digest(), ev.digest());
    assert_eq!(
        advisory.boundary(),
        ScenarioEvidenceOptimizerBoundary::AdvisoryOnly
    );
    assert!(!advisory.is_authoritative());
    assert!(!advisory.can_select_plan_alone());
    assert!(!advisory.can_drive_active_stats_version_transition());
}

#[test]
fn digest_is_deterministic_for_equal_records() {
    let a = evidence_at(500, 800, 100, 200).unwrap();
    let b = evidence_at(500, 800, 100, 200).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.digest(), b.digest());
}

#[test]
fn digest_separates_by_score_confidence_and_window() {
    let base = evidence_at(500, 800, 100, 200).unwrap();
    let other_score = evidence_at(501, 800, 100, 200).unwrap();
    let other_conf = evidence_at(500, 801, 100, 200).unwrap();
    let other_issued = evidence_at(500, 800, 101, 200).unwrap();
    let other_expiry = evidence_at(500, 800, 100, 201).unwrap();

    let d = base.digest();
    assert_ne!(d, other_score.digest());
    assert_ne!(d, other_conf.digest());
    assert_ne!(d, other_issued.digest());
    assert_ne!(d, other_expiry.digest());
}

#[test]
fn digest_separates_by_target_version_and_class() {
    let base = evidence_at(500, 800, 100, 200).unwrap();

    let mut other_stats_target = target();
    other_stats_target.stats_version = StatsVersion::new(4);
    let other_stats = evidence_with(
        42,
        ScenarioKind::Microbenchmark,
        other_stats_target,
        500,
        800,
        100,
        200,
    )
    .unwrap();

    let mut other_class_target = target();
    other_class_target.plan_class = Some(PlanClass::Cardinality);
    let other_class = evidence_with(
        42,
        ScenarioKind::Microbenchmark,
        other_class_target,
        500,
        800,
        100,
        200,
    )
    .unwrap();

    let mut absent_class_target = target();
    absent_class_target.plan_class = None;
    let absent_class = evidence_with(
        42,
        ScenarioKind::Microbenchmark,
        absent_class_target,
        500,
        800,
        100,
        200,
    )
    .unwrap();

    let mut absent_hash_target = target();
    absent_hash_target.contract_hash = None;
    let absent_hash = evidence_with(
        42,
        ScenarioKind::Microbenchmark,
        absent_hash_target,
        500,
        800,
        100,
        200,
    )
    .unwrap();

    let d = base.digest();
    let mut other_catalog_target = target();
    other_catalog_target.catalog_version = CatalogVersion::new(4);
    let other_catalog = evidence_with(
        42,
        ScenarioKind::Microbenchmark,
        other_catalog_target,
        500,
        800,
        100,
        200,
    )
    .unwrap();
    assert_ne!(d, other_catalog.digest());
    assert_ne!(d, other_stats.digest());
    assert_ne!(d, other_class.digest());
    // Absent vs Some(...) must differ from Some(other) and from each other.
    assert_ne!(d, absent_class.digest());
    assert_ne!(other_class.digest(), absent_class.digest());
    assert_ne!(d, absent_hash.digest());
}

#[test]
fn digest_separates_by_scenario_kind_and_id() {
    let base = evidence_at(500, 800, 100, 200).unwrap();

    let other_kind = evidence_with(
        42,
        ScenarioKind::RegressionProbe,
        target(),
        500,
        800,
        100,
        200,
    )
    .unwrap();

    let other_id = evidence_with(
        43,
        ScenarioKind::Microbenchmark,
        target(),
        500,
        800,
        100,
        200,
    )
    .unwrap();

    assert_ne!(base.digest(), other_kind.digest());
    assert_ne!(base.digest(), other_id.digest());
}
