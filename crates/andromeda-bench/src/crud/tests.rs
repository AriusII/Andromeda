use super::*;

#[test]
fn crud_scenario_definitions_are_valid() {
    for scenario in CRUD_SCENARIOS {
        assert!(
            scenario.validate().is_ok(),
            "scenario {} is invalid",
            scenario.id
        );
        assert!(!scenario.hypothesis.is_empty());
        assert_eq!(scenario.primary_metric, CRUD_BENCHMARK_PRIMARY_METRIC);
        assert_eq!(scenario.budget_origin, CRUD_BENCHMARK_BUDGET_ORIGIN);
        assert_eq!(scenario.decision_linkage, CRUD_BENCHMARK_DECISION_LINKAGE);
    }
}

#[test]
fn crud_data_generator_is_deterministic() {
    let gen1 = CrudDataGenerator::new(42, 128);
    let gen2 = CrudDataGenerator::new(42, 128);

    let rows1 = gen1.generate_rows(100);
    let rows2 = gen2.generate_rows(100);

    assert_eq!(rows1, rows2);
}

#[test]
fn crud_operation_metrics_can_serialize_to_json() {
    let result = CrudWorkloadResult {
        scenario_id: "crud-single-1".to_string(),
        start_time_unix_ms: 1000,
        elapsed_ms: 5000,
        thread_count: 1,
        batch_size: 1,
        row_count: 10_000,
        operations: vec![CrudOperationMetrics {
            operation: "insert".to_string(),
            count: 100,
            total_us: 50_000,
            p50_us: 500,
            p95_us: 1000,
            p99_us: 1500,
            throughput_ops_sec: 2000.0,
            error_count: 0,
        }],
        total_ops: 100,
        total_throughput_ops_sec: 2000.0,
        total_errors: 0,
        seed: 42,
    };

    let json = result.to_json();
    assert!(json.contains("\"schema\":\"andromeda.bench.crud.result.v1\""));
    assert!(json.contains("\"scenario_id\":\"crud-single-1\""));
    assert!(json.contains("\"operation\":\"insert\""));
    assert!(json.contains("\"authoritative\":false"));
    assert!(json.contains("\"can_select_plan_alone\":false"));
    assert!(json.contains("\"optimizer_boundary\":\"advisory-only\""));
    assert!(json.contains("\"scenario_metadata\":{"));
    assert!(json.contains("\"budget_origin\":\"static-crud-scenario-registry-v1\""));
    assert!(json.contains("\"decision_linkage\":\"advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass\""));
    assert!(json.contains("\"max_thread_count\":8"));
    assert!(json.contains("\"max_batch_size\":1000000"));
    assert!(json.contains("\"max_row_count\":1000000"));
    assert!(json.contains("\"max_duration_ms\":60000"));
    assert!(!result.is_authoritative());
    assert!(!result.can_select_plan_alone());
    assert_eq!(result.optimizer_consumption_role(), "advisory-only");
}

#[test]
fn crud_result_json_escapes_scenario_and_operation_names() {
    let result = CrudWorkloadResult {
        scenario_id: "crud\"synthetic".to_string(),
        start_time_unix_ms: 1000,
        elapsed_ms: 5000,
        thread_count: 1,
        batch_size: 1,
        row_count: 10_000,
        operations: vec![CrudOperationMetrics {
            operation: "insert\"diagnostic".to_string(),
            count: 100,
            total_us: 50_000,
            p50_us: 500,
            p95_us: 1000,
            p99_us: 1500,
            throughput_ops_sec: 2000.0,
            error_count: 0,
        }],
        total_ops: 100,
        total_throughput_ops_sec: 2000.0,
        total_errors: 0,
        seed: 42,
    };

    let json = result.to_json();

    assert!(json.contains(r#""scenario_id":"crud\"synthetic""#));
    assert!(json.contains(r#""operation":"insert\"diagnostic""#));
}

#[test]
fn compute_percentile_works() {
    let latencies = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert_eq!(compute_percentile(&latencies, 50.0), 5);
    assert_eq!(compute_percentile(&latencies, 95.0), 10);
    assert_eq!(compute_percentile(&latencies, 99.0), 10);
}

#[test]
fn all_six_crud_scenarios_exist() {
    let expected_scenarios = vec![
        "crud-single-1",
        "crud-single-100",
        "crud-multi4-10",
        "crud-multi8-100",
        "crud-scan-1m",
        "crud-write-heavy",
    ];

    for expected_id in expected_scenarios {
        let scenario = find_crud_scenario(expected_id);
        assert!(scenario.is_some(), "scenario {} should exist", expected_id);
    }

    assert_eq!(CRUD_SCENARIOS.len(), 6);
}

#[test]
fn crud_scenario_operation_percentages_sum_to_100() {
    for scenario in CRUD_SCENARIOS {
        let sum = u16::from(scenario.insert_pct)
            + u16::from(scenario.update_pct)
            + u16::from(scenario.delete_pct)
            + u16::from(scenario.scan_pct);
        assert_eq!(
            sum, 100,
            "scenario {} percentages should sum to 100",
            scenario.id
        );
    }
}

#[test]
fn data_generator_generates_expected_count() {
    let r#gen = CrudDataGenerator::new(42, 256);

    let rows = r#gen.generate_rows(1000);
    assert_eq!(rows.len(), 1000);

    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.timestamp, i as u64);
        assert_eq!(row.payload.len(), 256);
    }
}

#[test]
fn data_generator_different_seeds_differ() {
    let gen1 = CrudDataGenerator::new(42, 128);
    let gen2 = CrudDataGenerator::new(99, 128);

    let rows1 = gen1.generate_rows(100);
    let rows2 = gen2.generate_rows(100);

    assert!(!rows1.iter().zip(rows2.iter()).all(|(r1, r2)| r1 == r2));
}

#[test]
fn crud_workload_result_comprehensive_json_serialization() {
    let result = CrudWorkloadResult {
        scenario_id: "crud-multi4-10".to_string(),
        start_time_unix_ms: 1704067200000,
        elapsed_ms: 5000,
        thread_count: 4,
        batch_size: 10,
        row_count: 100_000,
        operations: vec![
            CrudOperationMetrics {
                operation: "insert".to_string(),
                count: 250,
                total_us: 125_000,
                p50_us: 500,
                p95_us: 1000,
                p99_us: 1500,
                throughput_ops_sec: 2000.0,
                error_count: 0,
            },
            CrudOperationMetrics {
                operation: "update".to_string(),
                count: 250,
                total_us: 150_000,
                p50_us: 600,
                p95_us: 1200,
                p99_us: 1800,
                throughput_ops_sec: 1666.67,
                error_count: 0,
            },
        ],
        total_ops: 500,
        total_throughput_ops_sec: 1833.34,
        total_errors: 0,
        seed: 99,
    };

    let json = result.to_json();
    assert!(json.contains("\"schema\":\"andromeda.bench.crud.result.v1\""));
    assert!(json.contains("\"scenario_id\":\"crud-multi4-10\""));
    assert!(json.contains("\"thread_count\":4"));
    assert!(json.contains("\"batch_size\":10"));
    assert!(json.contains("\"seed\":99"));
    assert!(json.contains("\"authoritative\":false"));
    assert!(json.contains("\"can_select_plan_alone\":false"));
    assert!(json.contains("\"optimizer_boundary\":\"advisory-only\""));
    assert!(json.contains("\"max_thread_count\":8"));
    assert!(json.contains("\"max_batch_size\":1000000"));
    assert!(json.contains("\"max_row_count\":1000000"));
    assert!(json.contains("\"max_duration_ms\":60000"));
}

#[test]
fn percentile_empty_slice_returns_zero() {
    let latencies: Vec<u64> = vec![];
    let p50 = compute_percentile(&latencies, 50.0);
    assert_eq!(p50, 0);
}

#[test]
fn percentile_single_element() {
    let latencies = vec![42];
    let p50 = compute_percentile(&latencies, 50.0);
    let p99 = compute_percentile(&latencies, 99.0);
    assert_eq!(p50, 42);
    assert_eq!(p99, 42);
}

#[test]
fn scenario_validation_enforces_constraints() {
    let mut scenario = find_crud_scenario("crud-single-1").unwrap().clone();
    scenario.thread_count = 0;
    assert!(scenario.validate().is_err());

    scenario.thread_count = 1;
    scenario.batch_size = 0;
    assert!(scenario.validate().is_err());

    scenario.batch_size = 1;
    scenario.row_count = 0;
    assert!(scenario.validate().is_err());

    scenario.row_count = 100;
    scenario.max_duration_ms = 0;
    assert!(scenario.validate().is_err());

    scenario.max_duration_ms = 1;
    scenario.insert_pct = 50;
    scenario.update_pct = 50;
    scenario.delete_pct = 10;
    scenario.scan_pct = 0;
    assert!(scenario.validate().is_err());
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
