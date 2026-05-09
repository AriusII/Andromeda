use andromeda_bench_workload::{
    CrudOperationMetrics, MAX_CRUD_BATCH_SIZE, MAX_CRUD_DURATION_MS, MAX_CRUD_ROWS,
    MAX_CRUD_THREADS, find_crud_scenario,
};

use crate::flat_json::escape_json_string;
use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
};

/// Complete advisory results from a CRUD workload run.
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
            .map(operation_metrics_json)
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

fn operation_metrics_json(op: &CrudOperationMetrics) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result(scenario_id: &str, operation: &str) -> CrudWorkloadResult {
        CrudWorkloadResult {
            scenario_id: scenario_id.to_string(),
            start_time_unix_ms: 1_000,
            elapsed_ms: 5_000,
            thread_count: 1,
            batch_size: 1,
            row_count: 10_000,
            operations: vec![CrudOperationMetrics {
                operation: operation.to_string(),
                count: 100,
                total_us: 50_000,
                p50_us: 500,
                p95_us: 1_000,
                p99_us: 1_500,
                throughput_ops_sec: 2_000.0,
                error_count: 0,
            }],
            total_ops: 100,
            total_throughput_ops_sec: 2_000.0,
            total_errors: 0,
            seed: 42,
        }
    }

    #[test]
    fn crud_workload_result_serializes_advisory_boundary() {
        let result = sample_result("crud-single-1", "insert");

        let json = result.to_json();

        assert!(json.contains("\"schema\":\"andromeda.bench.crud.result.v1\""));
        assert!(json.contains("\"scenario_id\":\"crud-single-1\""));
        assert!(json.contains("\"operation\":\"insert\""));
        assert!(json.contains("\"authoritative\":false"));
        assert!(json.contains("\"can_select_plan_alone\":false"));
        assert!(json.contains("\"optimizer_boundary\":\"advisory-only\""));
        assert!(json.contains("\"scenario_metadata\":{"));
        assert!(json.contains("\"max_thread_count\":8"));
        assert!(json.contains("\"max_batch_size\":1000000"));
        assert!(json.contains("\"max_row_count\":1000000"));
        assert!(json.contains("\"max_duration_ms\":60000"));
        assert!(!result.is_authoritative());
        assert!(!result.can_select_plan_alone());
        assert_eq!(result.optimizer_consumption_role(), "advisory-only");
    }

    #[test]
    fn crud_workload_result_json_escapes_names() {
        let result = sample_result("crud\"synthetic", "insert\"diagnostic");

        let json = result.to_json();

        assert!(json.contains(r#""scenario_id":"crud\"synthetic""#));
        assert!(json.contains(r#""operation":"insert\"diagnostic""#));
    }
}
