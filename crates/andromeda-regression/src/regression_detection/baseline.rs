use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkEvidence, BenchmarkScenarioTarget,
};
use andromeda_scenario_evidence::flat_json::{
    escape_json_string, json_optional_str, json_optional_u32, json_optional_u64, optional_string,
    parse_flat_json_object, required_string, required_u64,
};

use super::advisory_json::verify_advisory_fields;
use super::baseline_context::BenchmarkBaselineContext;
use super::errors::BenchmarkBaselineComparisonError;

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkBaseline {
    pub workload_id: String,
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub error_count: u32,
    pub sample_count: u32,
    pub established_at: String,
    pub source_commit_id: Option<String>,
    pub context: BenchmarkBaselineContext,
}

impl BenchmarkBaseline {
    /// Create a new baseline from current evidence.
    pub fn from_evidence(
        workload_id: String,
        p50_latency_us: u64,
        p95_latency_us: u64,
        error_count: u32,
        sample_count: u32,
        timestamp: String,
    ) -> Self {
        let context = BenchmarkBaselineContext::from_workload_registry(&workload_id);
        Self {
            workload_id,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            established_at: timestamp,
            source_commit_id: None,
            context,
        }
    }

    /// Create a baseline bound to scenario metadata, run budgets, hardware profile,
    /// build identity, and optimizer target tuple.
    pub fn from_benchmark_evidence(
        evidence: &BenchmarkEvidence,
        source_commit_id: impl Into<String>,
        established_at: impl Into<String>,
        target: BenchmarkScenarioTarget,
    ) -> Self {
        Self {
            workload_id: evidence.workload_id.clone(),
            p50_latency_us: evidence.p50_latency_us,
            p95_latency_us: evidence.p95_latency_us,
            error_count: evidence.error_count,
            sample_count: evidence.sample_count,
            established_at: established_at.into(),
            source_commit_id: Some(source_commit_id.into()),
            context: BenchmarkBaselineContext::from_benchmark_evidence(evidence, target),
        }
    }

    pub fn validate_comparable_to_evidence(
        &self,
        evidence: &BenchmarkEvidence,
        target: BenchmarkScenarioTarget,
    ) -> Result<(), BenchmarkBaselineComparisonError> {
        if self.workload_id != evidence.workload_id {
            return Err(BenchmarkBaselineComparisonError::WorkloadIdMismatch);
        }
        self.context
            .validate_comparable_to_evidence(evidence, target)
    }

    /// Serialize baseline to JSON for artifact storage.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"workload_id":"{}","p50_latency_us":{},"p95_latency_us":{},"error_count":{},"sample_count":{},"established_at":"{}","source_commit_id":{},"workload_shape_version":{},"workload_class":{},"baseline_ref":{},"hardware_profile":{},"measurement_mode":{},"duration_budget_ms":{},"sample_budget":{},"temp_budget_bytes":{},"target_procedure_id":{},"target_catalog_version":{},"target_contract_hash":{},"target_stats_version":{},"target_plan_class":{},"authoritative":{},"can_select_plan_alone":{},"optimizer_boundary":"{}"}}"#,
            escape_json_string(&self.workload_id),
            self.p50_latency_us,
            self.p95_latency_us,
            self.error_count,
            self.sample_count,
            escape_json_string(&self.established_at),
            json_optional_str(self.source_commit_id.as_deref()),
            json_optional_str(self.context.workload_shape_version.as_deref()),
            json_optional_str(self.context.workload_class.as_deref()),
            json_optional_str(self.context.baseline_ref.as_deref()),
            json_optional_str(self.context.hardware_profile.as_deref()),
            json_optional_str(self.context.measurement_mode.as_deref()),
            json_optional_u64(self.context.duration_budget_ms),
            json_optional_u32(self.context.sample_budget),
            json_optional_u64(self.context.temp_budget_bytes),
            json_optional_u64(self.context.target_procedure_id),
            json_optional_u64(self.context.target_catalog_version),
            json_optional_str(self.context.target_contract_hash.as_deref()),
            json_optional_u64(self.context.target_stats_version),
            json_optional_str(self.context.target_plan_class.as_deref()),
            BENCHMARK_EVIDENCE_AUTHORITATIVE,
            BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
            escape_json_string(BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY)
        )
    }

    /// Deserialize baseline from JSON.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let value = parse_flat_json_object(json_str)
            .map_err(|error| format!("invalid baseline JSON: {error}"))?;

        let workload_id = required_string(&value, "workload_id")?;
        let p50_latency_us = required_u64(&value, "p50_latency_us")?;
        let p95_latency_us = required_u64(&value, "p95_latency_us")?;
        let error_count = u32::try_from(required_u64(&value, "error_count")?)
            .map_err(|_| "error_count exceeds u32".to_string())?;
        let sample_count = u32::try_from(required_u64(&value, "sample_count")?)
            .map_err(|_| "sample_count exceeds u32".to_string())?;
        let established_at = required_string(&value, "established_at")?;
        let source_commit_id = optional_string(&value, "source_commit_id")?;
        verify_advisory_fields(&value)?;
        let context = BenchmarkBaselineContext::from_json_fields(&value)?;

        Ok(Self {
            workload_id,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            established_at,
            source_commit_id,
            context,
        })
    }
}
