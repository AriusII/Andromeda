/// CRUD workload contracts and scenarios for synthetic diagnostic benchmark execution.
///
/// This module provides bounded synthetic CRUD workload scaffolding with:
/// - Deterministic data generation (seeded PRNG)
/// - Configurable concurrency and batching
/// - Latency/throughput metrics collection
/// - 6 pre-defined diagnostic scenarios (single/multi-threaded, varying batch sizes)
use crate::evidence::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
};
use crate::flat_json::escape_json_string;

pub const MAX_CRUD_THREADS: usize = 8;
pub const MAX_CRUD_BATCH_SIZE: usize = 1_000_000;
pub const MAX_CRUD_ROWS: u32 = 1_000_000;
pub const MAX_CRUD_DURATION_MS: u64 = 60_000;
pub const CRUD_BENCHMARK_PRIMARY_METRIC: &str = "throughput_ops_sec,p50_us,p95_us,error_count";
pub const CRUD_BENCHMARK_BUDGET_ORIGIN: &str = "static-crud-scenario-registry-v1";
pub const CRUD_BENCHMARK_DECISION_LINKAGE: &str =
    "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudScenarioDefinition {
    pub id: &'static str,
    pub description: &'static str,
    pub hypothesis: &'static str,
    pub workload_shape_version: &'static str,
    pub workload_size: &'static str,
    pub primary_metric: &'static str,
    pub budget_origin: &'static str,
    pub decision_linkage: &'static str,
    pub thread_count: usize,
    pub batch_size: usize,
    pub row_count: u32,
    pub insert_pct: u8,
    pub update_pct: u8,
    pub delete_pct: u8,
    pub scan_pct: u8,
    pub max_duration_ms: u64,
}

impl CrudScenarioDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.thread_count == 0 {
            return Err("thread_count must be > 0");
        }
        if self.thread_count > MAX_CRUD_THREADS {
            return Err("thread_count exceeds global CRUD benchmark limit");
        }
        if self.batch_size == 0 {
            return Err("batch_size must be > 0");
        }
        if self.batch_size > MAX_CRUD_BATCH_SIZE {
            return Err("batch_size exceeds global CRUD benchmark limit");
        }
        if self.row_count == 0 {
            return Err("row_count must be > 0");
        }
        if self.row_count > MAX_CRUD_ROWS {
            return Err("row_count exceeds global CRUD benchmark limit");
        }
        if self.hypothesis.trim().is_empty() {
            return Err("hypothesis must not be empty");
        }
        if self.workload_shape_version.trim().is_empty() {
            return Err("workload_shape_version must not be empty");
        }
        if self.workload_size.trim().is_empty() {
            return Err("workload_size must not be empty");
        }
        if self.primary_metric != CRUD_BENCHMARK_PRIMARY_METRIC {
            return Err("primary_metric must match the CRUD benchmark metric contract");
        }
        if self.budget_origin != CRUD_BENCHMARK_BUDGET_ORIGIN {
            return Err("budget_origin must match the CRUD benchmark registry");
        }
        if self.decision_linkage != CRUD_BENCHMARK_DECISION_LINKAGE {
            return Err("decision_linkage must preserve advisory optimizer linkage");
        }
        if self.max_duration_ms == 0 {
            return Err("max_duration_ms must be > 0");
        }
        if self.max_duration_ms > MAX_CRUD_DURATION_MS {
            return Err("max_duration_ms exceeds global CRUD benchmark limit");
        }
        let pct_sum = u16::from(self.insert_pct)
            + u16::from(self.update_pct)
            + u16::from(self.delete_pct)
            + u16::from(self.scan_pct);
        if pct_sum != 100 {
            return Err("operation percentages must sum to 100");
        }
        Ok(())
    }
}

/// Six synthetic diagnostic scenarios spanning single/multi-threaded shape and operation mixes.
pub const CRUD_SCENARIOS: &[CrudScenarioDefinition] = &[
    CrudScenarioDefinition {
        id: "crud-single-1",
        description: "Single-threaded, batch size 1, balanced CRUD mix (10K rows)",
        hypothesis: "single-threaded balanced CRUD accounting should stay within diagnostic latency and error budgets",
        workload_shape_version: "crud-single-1.synthetic.v1",
        workload_size: "single thread, batch size 1, 10K deterministic rows, duration_ms<=30000",
        primary_metric: CRUD_BENCHMARK_PRIMARY_METRIC,
        budget_origin: CRUD_BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: CRUD_BENCHMARK_DECISION_LINKAGE,
        thread_count: 1,
        batch_size: 1,
        row_count: 10_000,
        insert_pct: 25,
        update_pct: 25,
        delete_pct: 25,
        scan_pct: 25,
        max_duration_ms: 30_000,
    },
    CrudScenarioDefinition {
        id: "crud-single-100",
        description: "Single-threaded, batch size 100, balanced CRUD mix (10K rows)",
        hypothesis: "single-threaded batched CRUD accounting should stay within diagnostic latency and error budgets",
        workload_shape_version: "crud-single-100.synthetic.v1",
        workload_size: "single thread, batch size 100, 10K deterministic rows, duration_ms<=30000",
        primary_metric: CRUD_BENCHMARK_PRIMARY_METRIC,
        budget_origin: CRUD_BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: CRUD_BENCHMARK_DECISION_LINKAGE,
        thread_count: 1,
        batch_size: 100,
        row_count: 10_000,
        insert_pct: 25,
        update_pct: 25,
        delete_pct: 25,
        scan_pct: 25,
        max_duration_ms: 30_000,
    },
    CrudScenarioDefinition {
        id: "crud-multi4-10",
        description: "4-threaded, batch size 10, balanced CRUD mix (100K rows)",
        hypothesis: "four-thread balanced CRUD accounting should stay within diagnostic latency and error budgets",
        workload_shape_version: "crud-multi4-10.synthetic.v1",
        workload_size: "four threads, batch size 10, 100K deterministic rows, duration_ms<=30000",
        primary_metric: CRUD_BENCHMARK_PRIMARY_METRIC,
        budget_origin: CRUD_BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: CRUD_BENCHMARK_DECISION_LINKAGE,
        thread_count: 4,
        batch_size: 10,
        row_count: 100_000,
        insert_pct: 25,
        update_pct: 25,
        delete_pct: 25,
        scan_pct: 25,
        max_duration_ms: 30_000,
    },
    CrudScenarioDefinition {
        id: "crud-multi8-100",
        description: "8-threaded, batch size 100, balanced CRUD mix (100K rows)",
        hypothesis: "eight-thread batched CRUD accounting should stay within diagnostic latency and error budgets",
        workload_shape_version: "crud-multi8-100.synthetic.v1",
        workload_size: "eight threads, batch size 100, 100K deterministic rows, duration_ms<=30000",
        primary_metric: CRUD_BENCHMARK_PRIMARY_METRIC,
        budget_origin: CRUD_BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: CRUD_BENCHMARK_DECISION_LINKAGE,
        thread_count: 8,
        batch_size: 100,
        row_count: 100_000,
        insert_pct: 25,
        update_pct: 25,
        delete_pct: 25,
        scan_pct: 25,
        max_duration_ms: 30_000,
    },
    CrudScenarioDefinition {
        id: "crud-scan-1m",
        description: "Single-threaded, batch size 1M, scan-heavy (70% scans, 1M rows)",
        hypothesis: "scan-heavy CRUD accounting over the maximum row budget should stay bounded and advisory-only",
        workload_shape_version: "crud-scan-1m.synthetic.v1",
        workload_size: "single thread, batch size 1M, 1M deterministic row budget, duration_ms<=60000",
        primary_metric: CRUD_BENCHMARK_PRIMARY_METRIC,
        budget_origin: CRUD_BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: CRUD_BENCHMARK_DECISION_LINKAGE,
        thread_count: 1,
        batch_size: 1_000_000,
        row_count: 1_000_000,
        insert_pct: 10,
        update_pct: 10,
        delete_pct: 10,
        scan_pct: 70,
        max_duration_ms: 60_000,
    },
    CrudScenarioDefinition {
        id: "crud-write-heavy",
        description: "4-threaded, batch size 10, write-heavy (40% insert, 40% update, 50K rows)",
        hypothesis: "write-heavy CRUD accounting should stay within bounded diagnostic budgets without becoming optimizer authority",
        workload_shape_version: "crud-write-heavy.synthetic.v1",
        workload_size: "four threads, batch size 10, 50K deterministic rows, duration_ms<=30000",
        primary_metric: CRUD_BENCHMARK_PRIMARY_METRIC,
        budget_origin: CRUD_BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: CRUD_BENCHMARK_DECISION_LINKAGE,
        thread_count: 4,
        batch_size: 10,
        row_count: 50_000,
        insert_pct: 40,
        update_pct: 40,
        delete_pct: 20,
        scan_pct: 0,
        max_duration_ms: 30_000,
    },
];

pub fn find_crud_scenario(id: &str) -> Option<&'static CrudScenarioDefinition> {
    CRUD_SCENARIOS.iter().find(|s| s.id == id)
}

/// Deterministic data generator for reproducible benchmarks.
#[derive(Debug, Clone)]
pub struct CrudDataGenerator {
    seed: u64,
    row_size_bytes: usize,
}

impl CrudDataGenerator {
    pub fn new(seed: u64, row_size_bytes: usize) -> Self {
        Self {
            seed,
            row_size_bytes: row_size_bytes.max(64),
        }
    }

    /// Generate deterministic rows using XorShift64 PRNG.
    pub fn generate_rows(&self, count: u32) -> Vec<CrudRow> {
        let mut rows = Vec::with_capacity(count as usize);
        let mut state = self.seed;

        for i in 0..count {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let payload = vec![((state >> (i % 64)) & 0xFF) as u8; self.row_size_bytes];
            rows.push(CrudRow {
                id: state,
                payload,
                timestamp: u64::from(i),
            });
        }

        rows
    }
}

/// A row in the CRUD benchmark workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrudRow {
    pub id: u64,
    pub payload: Vec<u8>,
    pub timestamp: u64,
}

/// Metrics collected for a single operation type.
#[derive(Debug, Clone, PartialEq)]
pub struct CrudOperationMetrics {
    pub operation: String,
    pub count: u64,
    pub total_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub throughput_ops_sec: f64,
    pub error_count: u32,
}

/// Complete results from a CRUD workload run.
#[derive(Debug, Clone, PartialEq)]
pub struct CrudWorkloadResult {
    pub scenario_id: String,
    pub start_time_unix_ms: u64,
    pub elapsed_ms: u64,
    pub thread_count: usize,
    pub batch_size: usize,
    pub row_count: u32,
    pub operations: Vec<CrudOperationMetrics>,
    pub total_ops: u64,
    pub total_throughput_ops_sec: f64,
    pub total_errors: u32,
    pub seed: u64,
}

impl CrudWorkloadResult {
    /// CRUD benchmark results are diagnostics, not optimizer decisions.
    pub const fn is_authoritative(&self) -> bool {
        BENCHMARK_EVIDENCE_AUTHORITATIVE
    }

    /// CRUD benchmark results can never select a plan by themselves.
    pub const fn can_select_plan_alone(&self) -> bool {
        BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE
    }

    pub const fn optimizer_consumption_role(&self) -> &'static str {
        BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY
    }

    pub fn to_json(&self) -> String {
        let ops_json = self
            .operations
            .iter()
            .map(|op| {
                format!(
                    "{{\"operation\":\"{}\",\"count\":{},\"total_us\":{},\"p50_us\":{},\"p95_us\":{},\"p99_us\":{},\"throughput_ops_sec\":{:.2},\"error_count\":{}}}",
                    escape_json_string(&op.operation),
                    op.count,
                    op.total_us,
                    op.p50_us,
                    op.p95_us,
                    op.p99_us,
                    op.throughput_ops_sec,
                    op.error_count
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let scenario_metadata = scenario_metadata_json(&self.scenario_id);

        format!(
            r#"{{"schema":"andromeda.bench.crud.result.v1","diagnostic_only":true,"authoritative":{},"can_select_plan_alone":{},"optimizer_boundary":"{}","scenario_metadata":{},"max_thread_count":{},"max_batch_size":{},"max_row_count":{},"max_duration_ms":{},"scenario_id":"{}","start_time_unix_ms":{},"elapsed_ms":{},"thread_count":{},"batch_size":{},"row_count":{},"operations":[{}],"total_ops":{},"total_throughput_ops_sec":{:.2},"total_errors":{},"seed":{}}}"#,
            self.is_authoritative(),
            self.can_select_plan_alone(),
            self.optimizer_consumption_role(),
            scenario_metadata,
            MAX_CRUD_THREADS,
            MAX_CRUD_BATCH_SIZE,
            MAX_CRUD_ROWS,
            MAX_CRUD_DURATION_MS,
            escape_json_string(&self.scenario_id),
            self.start_time_unix_ms,
            self.elapsed_ms,
            self.thread_count,
            self.batch_size,
            self.row_count,
            ops_json,
            self.total_ops,
            self.total_throughput_ops_sec,
            self.total_errors,
            self.seed
        )
    }
}

fn scenario_metadata_json(scenario_id: &str) -> String {
    let Some(scenario) = find_crud_scenario(scenario_id) else {
        return "null".to_string();
    };

    format!(
        r#"{{"hypothesis":"{}","workload_shape_version":"{}","workload_size":"{}","primary_metric":"{}","budget_origin":"{}","decision_linkage":"{}"}}"#,
        escape_json_string(scenario.hypothesis),
        escape_json_string(scenario.workload_shape_version),
        escape_json_string(scenario.workload_size),
        escape_json_string(scenario.primary_metric),
        escape_json_string(scenario.budget_origin),
        escape_json_string(scenario.decision_linkage)
    )
}

/// Computes percentiles from a sorted slice of latencies.
pub fn compute_percentile(sorted_latencies: &[u64], percentile: f64) -> u64 {
    if sorted_latencies.is_empty() {
        return 0;
    }
    let rank = (percentile / 100.0 * sorted_latencies.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1);
    sorted_latencies[idx.min(sorted_latencies.len() - 1)]
}

#[cfg(test)]
mod tests {
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
}
