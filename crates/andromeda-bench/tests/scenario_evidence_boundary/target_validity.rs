use andromeda_bench::{
    BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence, BenchmarkEvidenceValidity,
    BenchmarkPlanClass, BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget,
    BenchmarkStatsVersion,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::common::{
    boundary_from_history, default_budgets, default_confidence, target, target_with_stats, ts,
    vertical_history_record,
};

#[test]
fn boundary_rejects_missing_contract_hash_and_stale_stats_version() {
    assert_eq!(
        BenchmarkScenarioTarget::new(
            ProcedureId::new(0x5253),
            CatalogVersion::new(9),
            ContractHash::zero(),
            BenchmarkStatsVersion::new(1),
            BenchmarkPlanClass::StatsAdaptive,
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetContractHashZero
    );

    let record = vertical_history_record(20);
    let boundary = boundary_from_history(&record, default_budgets()).unwrap();

    assert_eq!(
        boundary
            .validate_for_target_at(target_with_stats(2), ts(2_000))
            .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetStatsVersionMismatch
    );
}

#[test]
fn boundary_validity_window_is_half_open_and_expirable() {
    let record = vertical_history_record(20);
    let short_validity =
        andromeda_bench::BenchmarkEvidenceValidity::new(ts(1_000), ts(2_000)).unwrap();
    let boundary = record
        .to_scenario_evidence_boundary(
            target(),
            default_budgets(),
            default_confidence(),
            short_validity,
        )
        .unwrap();

    assert_eq!(
        boundary.validate_for_use_at(ts(999)).unwrap_err(),
        BenchmarkScenarioEvidenceError::NotYetValid
    );
    assert!(boundary.validate_for_use_at(ts(1_000)).is_ok());
    assert_eq!(
        boundary
            .validate_for_target_at(target(), ts(2_000))
            .unwrap_err(),
        BenchmarkScenarioEvidenceError::Expired
    );
}

#[test]
fn boundary_confidence_and_validity_are_explicitly_bounded() {
    let record = vertical_history_record(20);
    let max_confidence = BenchmarkEvidenceConfidence::from_permille(1_000).unwrap();
    let short_validity = BenchmarkEvidenceValidity::new(ts(10), ts(20)).unwrap();
    let boundary = record
        .to_scenario_evidence_boundary(target(), default_budgets(), max_confidence, short_validity)
        .unwrap();

    assert_eq!(boundary.confidence().permille(), 1_000);
    assert_eq!(boundary.validity().issued_at().as_unix_millis(), 10);
    assert_eq!(boundary.validity().expires_at().as_unix_millis(), 20);
    assert_eq!(
        BenchmarkEvidenceConfidence::from_permille(1_001).unwrap_err(),
        BenchmarkScenarioEvidenceError::ConfidenceOutOfRange
    );

    let json = boundary.to_json();
    assert!(json.contains(r#""confidence_permille":1000"#));
    assert!(json.contains(r#""issued_at_unix_ms":10"#));
    assert!(json.contains(r#""expires_at_unix_ms":20"#));
}

#[test]
fn boundary_rejects_unversioned_or_unbounded_plan_cache_inputs() {
    let target_error = BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(0),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(1),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap_err();
    assert_eq!(
        target_error,
        BenchmarkScenarioEvidenceError::TargetCatalogVersionZero
    );

    let stats_error = BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(9),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(0),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap_err();
    assert_eq!(
        stats_error,
        BenchmarkScenarioEvidenceError::TargetStatsVersionZero
    );

    assert_eq!(
        BenchmarkEvidenceBudgets::new(5_000, 20, 0).unwrap_err(),
        BenchmarkScenarioEvidenceError::ZeroTempBudget
    );
}

#[test]
fn boundary_validity_window_is_half_open_and_explicit() {
    let record = vertical_history_record(20);

    let boundary = boundary_from_history(&record, default_budgets()).unwrap();

    assert_eq!(
        boundary.validate_for_use_at(ts(999)).unwrap_err(),
        BenchmarkScenarioEvidenceError::NotYetValid
    );
    assert!(boundary.validate_for_use_at(ts(1_000)).is_ok());
    assert_eq!(
        boundary.validate_for_use_at(ts(86_401_000)).unwrap_err(),
        BenchmarkScenarioEvidenceError::Expired
    );
}
