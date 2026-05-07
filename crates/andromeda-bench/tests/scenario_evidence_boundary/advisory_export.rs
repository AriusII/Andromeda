use andromeda_bench::{
    BenchmarkPlanClass, BenchmarkStatsVersion, BenchmarkWorkloadClass, DEFAULT_TEMP_BYTES,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::common::{boundary_from_history, default_budgets, ts, vertical_history_record};

#[test]
fn history_record_exports_non_authoritative_scenario_boundary() {
    let record = vertical_history_record(20);

    let boundary = boundary_from_history(&record, default_budgets()).unwrap();

    assert!(!boundary.is_authoritative());
    assert!(!boundary.can_select_plan_alone());
    assert_eq!(boundary.workload_id(), "vertical-v0-smoke");
    assert_eq!(boundary.commit_id(), "commit-20260506");
    assert_eq!(boundary.target().procedure_id, ProcedureId::new(0x5253));
    assert_eq!(boundary.target().catalog_version, CatalogVersion::new(9));
    assert_eq!(
        boundary.target().contract_hash,
        ContractHash::test_vector(0x52)
    );
    assert_eq!(
        boundary.target().stats_version,
        BenchmarkStatsVersion::new(1)
    );
    assert_eq!(
        boundary.target().plan_class,
        BenchmarkPlanClass::StatsAdaptive
    );
    assert_eq!(boundary.budgets().duration_ms, 5_000);
    assert_eq!(boundary.budgets().samples, 20);
    assert_eq!(boundary.budgets().temp_bytes, DEFAULT_TEMP_BYTES);
    assert_eq!(boundary.confidence().permille(), 850);
    assert_eq!(
        boundary.context().workload_class(),
        BenchmarkWorkloadClass::SyntheticDiagnostic
    );
    assert_eq!(boundary.context().hardware_profile(), None);
    assert!(boundary.validate_for_use_at(ts(2_000)).is_ok());

    let json = boundary.to_json();
    assert!(json.contains(r#""authoritative":false"#));
    assert!(json.contains(r#""can_select_plan_alone":false"#));
    assert!(json.contains(r#""optimizer_boundary":"advisory-only""#));
    assert!(json.contains(r#""target_plan_class":"stats-adaptive""#));
    assert!(json.contains(r#""target_procedure_id":21075"#));
    assert!(json.contains(r#""target_catalog_version":9"#));
    assert!(json.contains(r#""target_stats_version":1"#));
    assert!(json.contains(r#""hardware_profile":null"#));
}
