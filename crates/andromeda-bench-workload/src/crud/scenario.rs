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
        self.validate_resource_bounds()?;
        self.validate_question_metadata()?;
        self.validate_operation_mix()
    }

    fn validate_resource_bounds(&self) -> Result<(), &'static str> {
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
        if self.max_duration_ms == 0 {
            return Err("max_duration_ms must be > 0");
        }
        if self.max_duration_ms > MAX_CRUD_DURATION_MS {
            return Err("max_duration_ms exceeds global CRUD benchmark limit");
        }
        Ok(())
    }

    fn validate_question_metadata(&self) -> Result<(), &'static str> {
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
        Ok(())
    }

    fn validate_operation_mix(&self) -> Result<(), &'static str> {
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
    CRUD_SCENARIOS.iter().find(|scenario| scenario.id == id)
}
