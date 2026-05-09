use andromeda_bench_workload::{
    CRUD_BENCHMARK_BUDGET_ORIGIN, CRUD_BENCHMARK_DECISION_LINKAGE, CRUD_BENCHMARK_PRIMARY_METRIC,
    CRUD_SCENARIOS, CrudDataGenerator, MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS, MAX_CRUD_ROWS,
    MAX_CRUD_THREADS, compute_percentile, find_crud_scenario,
};

#[test]
fn crud_scenario_definitions_are_valid() {
    for scenario in CRUD_SCENARIOS {
        assert!(
            scenario.validate().is_ok(),
            "scenario {} is invalid",
            scenario.id
        );
        assert_eq!(scenario.primary_metric, CRUD_BENCHMARK_PRIMARY_METRIC);
        assert_eq!(scenario.budget_origin, CRUD_BENCHMARK_BUDGET_ORIGIN);
        assert_eq!(scenario.decision_linkage, CRUD_BENCHMARK_DECISION_LINKAGE);
    }
}

#[test]
fn crud_data_generator_is_deterministic() {
    let gen1 = CrudDataGenerator::new(42, 128);
    let gen2 = CrudDataGenerator::new(42, 128);

    assert_eq!(gen1.generate_rows(100), gen2.generate_rows(100));
}

#[test]
fn compute_percentile_uses_nearest_rank() {
    let latencies = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    assert_eq!(compute_percentile(&latencies, 50.0), 5);
    assert_eq!(compute_percentile(&latencies, 95.0), 10);
    assert_eq!(compute_percentile(&[], 50.0), 0);
}

#[test]
fn all_six_crud_scenarios_exist() {
    let expected_scenarios = [
        "crud-single-1",
        "crud-single-100",
        "crud-multi4-10",
        "crud-multi8-100",
        "crud-scan-1m",
        "crud-write-heavy",
    ];

    for expected_id in expected_scenarios {
        assert!(
            find_crud_scenario(expected_id).is_some(),
            "scenario {expected_id} should exist"
        );
    }

    assert_eq!(CRUD_SCENARIOS.len(), 6);
}

#[test]
fn scenario_validation_enforces_global_resource_caps() {
    let mut scenario = find_crud_scenario("crud-single-1").unwrap().clone();
    scenario.thread_count = MAX_CRUD_THREADS + 1;
    assert_eq!(
        scenario.validate().unwrap_err(),
        "thread_count exceeds global CRUD benchmark limit"
    );

    scenario.thread_count = 1;
    scenario.batch_size = MAX_CRUD_BATCH_SIZE + 1;
    assert_eq!(
        scenario.validate().unwrap_err(),
        "batch_size exceeds global CRUD benchmark limit"
    );

    scenario.batch_size = 1;
    scenario.row_count = MAX_CRUD_ROWS + 1;
    assert_eq!(
        scenario.validate().unwrap_err(),
        "row_count exceeds global CRUD benchmark limit"
    );

    scenario.row_count = 1;
    scenario.max_duration_ms = MAX_CRUD_DURATION_MS + 1;
    assert_eq!(
        scenario.validate().unwrap_err(),
        "max_duration_ms exceeds global CRUD benchmark limit"
    );
}
