use crate::flat_json::{
    escape_json_string, optional_string, optional_u32, parse_flat_json_object, required_string,
    required_u64,
};
use crate::{
    BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence, BenchmarkEvidenceValidity,
    BenchmarkScenarioEvidence, BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget,
};

use super::advisory::BenchmarkHistoryAdvisoryMetadata;
use super::json::{json_optional_string, json_optional_u32, required_u32_field};

/// A single benchmark run record in history.
///
/// Captures all evidence from a benchmark run at a specific point in time (commit).
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkHistoryRecord {
    /// Workload identifier (e.g., "btree-lookup-smoke")
    pub workload_id: String,
    /// Commit hash or build ID identifying this run
    pub commit_id: String,
    /// ISO 8601 timestamp when benchmark was executed
    pub timestamp: String,
    /// P50 latency in microseconds
    pub p50_latency_us: u64,
    /// P95 latency in microseconds
    pub p95_latency_us: u64,
    /// Error count during benchmark
    pub error_count: u32,
    /// Total samples collected
    pub sample_count: u32,
    /// Git branch name (optional, for workflow context)
    pub branch: Option<String>,
    /// PR number if this was a PR check (optional)
    pub pr_number: Option<u32>,
    /// Advisory-only optimizer consumption boundary and resource caps.
    pub advisory: BenchmarkHistoryAdvisoryMetadata,
}

impl BenchmarkHistoryRecord {
    pub fn new(
        workload_id: String,
        commit_id: String,
        timestamp: String,
        p50_latency_us: u64,
        p95_latency_us: u64,
        error_count: u32,
        sample_count: u32,
    ) -> Self {
        Self {
            workload_id,
            commit_id,
            timestamp,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            branch: None,
            pr_number: None,
            advisory: BenchmarkHistoryAdvisoryMetadata::advisory_only_global_caps(),
        }
    }

    /// Attach git context metadata to the record.
    pub fn with_context(mut self, branch: Option<String>, pr_number: Option<u32>) -> Self {
        self.branch = branch;
        self.pr_number = pr_number;
        self
    }

    /// Attach explicit advisory metadata and resource caps to the record.
    pub fn with_advisory_metadata(mut self, advisory: BenchmarkHistoryAdvisoryMetadata) -> Self {
        self.advisory = advisory;
        self
    }

    /// Convert this immutable history record into a bounded, advisory
    /// ScenarioEvidence boundary record.
    ///
    /// The bench crate does not construct catalog `ScenarioEvidence` directly.
    /// Callers must supply the target and budgets that bind this history point
    /// to a Procedure, ContractHash, StatsVersion, and PlanClass.
    pub fn to_scenario_evidence_boundary(
        &self,
        target: BenchmarkScenarioTarget,
        budgets: BenchmarkEvidenceBudgets,
        confidence: BenchmarkEvidenceConfidence,
        validity: BenchmarkEvidenceValidity,
    ) -> Result<BenchmarkScenarioEvidence, BenchmarkScenarioEvidenceError> {
        BenchmarkScenarioEvidence::from_history_record(self, target, budgets, confidence, validity)
    }

    /// Serialize record to JSON Line (one line, complete JSON object).
    ///
    /// JSON Lines format is chosen because it is portable across storage backends
    /// and supports append-without-parsing of the whole file.
    pub fn to_json_line(&self) -> String {
        let branch = json_optional_string(self.branch.as_deref());
        let pr_number = json_optional_u32(self.pr_number);

        format!(
            r#"{{"workload_id":"{}","commit_id":"{}","timestamp":"{}","p50_latency_us":{},"p95_latency_us":{},"error_count":{},"sample_count":{},"branch":{},"pr_number":{},"advisory_boundary":"{}","authoritative":{},"can_select_plan_alone":{},"optimizer_boundary":"{}","duration_cap_ms":{},"sample_cap":{},"temp_cap_bytes":{}}}"#,
            escape_json_string(&self.workload_id),
            escape_json_string(&self.commit_id),
            escape_json_string(&self.timestamp),
            self.p50_latency_us,
            self.p95_latency_us,
            self.error_count,
            self.sample_count,
            branch,
            pr_number,
            escape_json_string(&self.advisory.advisory_boundary),
            self.advisory.authoritative,
            self.advisory.can_select_plan_alone,
            escape_json_string(&self.advisory.optimizer_boundary),
            self.advisory.duration_cap_ms,
            self.advisory.sample_cap,
            self.advisory.temp_cap_bytes
        )
    }

    /// Deserialize from JSON Line format.
    pub fn from_json_line(line: &str) -> Result<Self, String> {
        let value = parse_flat_json_object(line)
            .map_err(|error| format!("invalid history JSON: {error}"))?;

        let workload_id = required_string(&value, "workload_id")?;
        let commit_id = required_string(&value, "commit_id")?;
        let timestamp = required_string(&value, "timestamp")?;
        let p50_latency_us = required_u64(&value, "p50_latency_us")?;
        let p95_latency_us = required_u64(&value, "p95_latency_us")?;
        let error_count = required_u32_field(&value, "error_count")?;
        let sample_count = required_u32_field(&value, "sample_count")?;
        let branch = optional_string(&value, "branch")?;
        let pr_number = optional_u32(&value, "pr_number")?;
        let advisory = BenchmarkHistoryAdvisoryMetadata::from_json_fields(&value)?;

        Ok(Self {
            workload_id,
            commit_id,
            timestamp,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            branch,
            pr_number,
            advisory,
        })
    }
}
