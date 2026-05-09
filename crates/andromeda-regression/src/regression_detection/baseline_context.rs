use std::collections::HashMap;

use crate::{BenchmarkEvidence, BenchmarkScenarioTarget, find_workload};
use andromeda_scenario_evidence::flat_json::{
    JsonField, optional_string, optional_u32, optional_u64,
};

use super::errors::BenchmarkBaselineComparisonError;
use super::mismatch_rejection::{compare_optional_str, compare_optional_u32, compare_optional_u64};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BenchmarkBaselineContext {
    pub workload_shape_version: Option<String>,
    pub workload_class: Option<String>,
    pub baseline_ref: Option<String>,
    pub hardware_profile: Option<String>,
    pub measurement_mode: Option<String>,
    pub duration_budget_ms: Option<u64>,
    pub sample_budget: Option<u32>,
    pub temp_budget_bytes: Option<u64>,
    pub target_procedure_id: Option<u64>,
    pub target_catalog_version: Option<u64>,
    pub target_contract_hash: Option<String>,
    pub target_stats_version: Option<u64>,
    pub target_plan_class: Option<String>,
}

impl BenchmarkBaselineContext {
    pub fn from_workload_registry(workload_id: &str) -> Self {
        let mut context = Self::default();
        if let Some(workload) = find_workload(workload_id) {
            context.workload_shape_version = Some(workload.workload_shape_version.to_string());
            context.workload_class = Some(workload.workload_class.as_str().to_string());
            context.baseline_ref = Some(workload.baseline_ref.to_string());
        }
        context
    }

    pub fn from_benchmark_evidence(
        evidence: &BenchmarkEvidence,
        target: BenchmarkScenarioTarget,
    ) -> Self {
        Self {
            workload_shape_version: Some(evidence.workload_shape_version.to_string()),
            workload_class: find_workload(&evidence.workload_id)
                .map(|workload| workload.workload_class.as_str().to_string()),
            baseline_ref: Some(evidence.baseline_ref.to_string()),
            hardware_profile: Some(evidence.hardware_profile.as_str().to_string()),
            measurement_mode: Some(evidence.measurement_mode.as_str().to_string()),
            duration_budget_ms: Some(evidence.duration_ms),
            sample_budget: Some(evidence.samples),
            temp_budget_bytes: Some(evidence.temp_budget_bytes),
            target_procedure_id: Some(target.procedure_id.get()),
            target_catalog_version: Some(target.catalog_version.get()),
            target_contract_hash: Some(target.contract_hash.to_string()),
            target_stats_version: Some(target.stats_version.get()),
            target_plan_class: Some(target.plan_class.as_str().to_string()),
        }
    }

    pub(super) fn from_json_fields(fields: &HashMap<String, JsonField>) -> Result<Self, String> {
        Ok(Self {
            workload_shape_version: optional_string(fields, "workload_shape_version")?,
            workload_class: optional_string(fields, "workload_class")?,
            baseline_ref: optional_string(fields, "baseline_ref")?,
            hardware_profile: optional_string(fields, "hardware_profile")?,
            measurement_mode: optional_string(fields, "measurement_mode")?,
            duration_budget_ms: optional_u64(fields, "duration_budget_ms")?,
            sample_budget: optional_u32(fields, "sample_budget")?,
            temp_budget_bytes: optional_u64(fields, "temp_budget_bytes")?,
            target_procedure_id: optional_u64(fields, "target_procedure_id")?,
            target_catalog_version: optional_u64(fields, "target_catalog_version")?,
            target_contract_hash: optional_string(fields, "target_contract_hash")?,
            target_stats_version: optional_u64(fields, "target_stats_version")?,
            target_plan_class: optional_string(fields, "target_plan_class")?,
        })
    }

    pub(super) fn validate_comparable_to_evidence(
        &self,
        evidence: &BenchmarkEvidence,
        target: BenchmarkScenarioTarget,
    ) -> Result<(), BenchmarkBaselineComparisonError> {
        compare_optional_str(
            self.workload_shape_version.as_deref(),
            evidence.workload_shape_version,
            BenchmarkBaselineComparisonError::WorkloadShapeVersionMismatch,
        )?;
        let workload = find_workload(&evidence.workload_id)
            .ok_or(BenchmarkBaselineComparisonError::WorkloadClassMismatch)?;
        compare_optional_str(
            self.workload_class.as_deref(),
            workload.workload_class.as_str(),
            BenchmarkBaselineComparisonError::WorkloadClassMismatch,
        )?;
        compare_optional_str(
            self.baseline_ref.as_deref(),
            evidence.baseline_ref,
            BenchmarkBaselineComparisonError::BaselineRefMismatch,
        )?;
        compare_optional_str(
            self.hardware_profile.as_deref(),
            evidence.hardware_profile.as_str(),
            BenchmarkBaselineComparisonError::HardwareProfileMismatch,
        )?;
        compare_optional_str(
            self.measurement_mode.as_deref(),
            evidence.measurement_mode.as_str(),
            BenchmarkBaselineComparisonError::MeasurementModeMismatch,
        )?;
        compare_optional_u64(
            self.duration_budget_ms,
            evidence.duration_ms,
            BenchmarkBaselineComparisonError::DurationBudgetMismatch,
        )?;
        compare_optional_u32(
            self.sample_budget,
            evidence.samples,
            BenchmarkBaselineComparisonError::SampleBudgetMismatch,
        )?;
        compare_optional_u64(
            self.temp_budget_bytes,
            evidence.temp_budget_bytes,
            BenchmarkBaselineComparisonError::TempBudgetMismatch,
        )?;
        compare_optional_u64(
            self.target_procedure_id,
            target.procedure_id.get(),
            BenchmarkBaselineComparisonError::TargetProcedureIdMismatch,
        )?;
        compare_optional_u64(
            self.target_catalog_version,
            target.catalog_version.get(),
            BenchmarkBaselineComparisonError::TargetCatalogVersionMismatch,
        )?;
        compare_optional_str(
            self.target_contract_hash.as_deref(),
            &target.contract_hash.to_string(),
            BenchmarkBaselineComparisonError::TargetContractHashMismatch,
        )?;
        compare_optional_u64(
            self.target_stats_version,
            target.stats_version.get(),
            BenchmarkBaselineComparisonError::TargetStatsVersionMismatch,
        )?;
        compare_optional_str(
            self.target_plan_class.as_deref(),
            target.plan_class.as_str(),
            BenchmarkBaselineComparisonError::TargetPlanClassMismatch,
        )?;
        Ok(())
    }
}
