use andromeda_core::EngineTimestamp;

use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkEvidence, BenchmarkHistoryRecord,
};

use super::{
    budgets::{
        BenchmarkEvidenceBudgets, validate_budget_against_workload,
        validate_history_advisory_metadata,
    },
    confidence::BenchmarkEvidenceConfidence,
    context::BenchmarkEvidenceContext,
    errors::BenchmarkScenarioEvidenceError,
    json,
    target::BenchmarkScenarioTarget,
    validity::BenchmarkEvidenceValidity,
};

/// Non-authoritative benchmark evidence ready for a catalog integration layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkScenarioEvidence {
    workload_id: String,
    commit_id: String,
    observed_at: String,
    p50_latency_us: u64,
    p95_latency_us: u64,
    error_count: u32,
    sample_count: u32,
    target: BenchmarkScenarioTarget,
    budgets: BenchmarkEvidenceBudgets,
    confidence: BenchmarkEvidenceConfidence,
    validity: BenchmarkEvidenceValidity,
    context: BenchmarkEvidenceContext,
}

impl BenchmarkScenarioEvidence {
    pub fn from_history_record(
        record: &BenchmarkHistoryRecord,
        target: BenchmarkScenarioTarget,
        budgets: BenchmarkEvidenceBudgets,
        confidence: BenchmarkEvidenceConfidence,
        validity: BenchmarkEvidenceValidity,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        let context = BenchmarkEvidenceContext::from_history_record(&record.workload_id)?;
        validate_history_advisory_metadata(record, budgets)?;
        Self::from_observation(
            &record.workload_id,
            &record.commit_id,
            &record.timestamp,
            record.p50_latency_us,
            record.p95_latency_us,
            record.error_count,
            record.sample_count,
            target,
            budgets,
            confidence,
            validity,
            context,
        )
    }

    pub fn from_benchmark_evidence(
        evidence: &BenchmarkEvidence,
        commit_id: impl Into<String>,
        observed_at: impl Into<String>,
        target: BenchmarkScenarioTarget,
        confidence: BenchmarkEvidenceConfidence,
        validity: BenchmarkEvidenceValidity,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        let budgets = BenchmarkEvidenceBudgets::new(
            evidence.duration_ms,
            evidence.samples,
            evidence.temp_budget_bytes,
        )?;
        let context = BenchmarkEvidenceContext::from_benchmark_evidence(evidence)?;
        let commit_id = commit_id.into();
        let observed_at = observed_at.into();
        Self::from_observation(
            &evidence.workload_id,
            &commit_id,
            &observed_at,
            evidence.p50_latency_us,
            evidence.p95_latency_us,
            evidence.error_count,
            evidence.sample_count,
            target,
            budgets,
            confidence,
            validity,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_observation(
        workload_id: &str,
        commit_id: &str,
        observed_at: &str,
        p50_latency_us: u64,
        p95_latency_us: u64,
        error_count: u32,
        sample_count: u32,
        target: BenchmarkScenarioTarget,
        budgets: BenchmarkEvidenceBudgets,
        confidence: BenchmarkEvidenceConfidence,
        validity: BenchmarkEvidenceValidity,
        context: BenchmarkEvidenceContext,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if workload_id.trim().is_empty() {
            return Err(BenchmarkScenarioEvidenceError::EmptyWorkloadId);
        }
        validate_budget_against_workload(workload_id, budgets)?;
        if commit_id.trim().is_empty() {
            return Err(BenchmarkScenarioEvidenceError::EmptyCommitId);
        }
        if observed_at.trim().is_empty() {
            return Err(BenchmarkScenarioEvidenceError::EmptyObservedAt);
        }
        if sample_count == 0 {
            return Err(BenchmarkScenarioEvidenceError::RecordSampleCountZero);
        }
        if sample_count > budgets.samples {
            return Err(BenchmarkScenarioEvidenceError::RecordSampleCountExceedsBudget);
        }
        if error_count > sample_count {
            return Err(BenchmarkScenarioEvidenceError::ErrorCountExceedsSampleCount);
        }
        if p95_latency_us < p50_latency_us {
            return Err(BenchmarkScenarioEvidenceError::LatencyPercentileOrderInvalid);
        }
        target.validate()?;
        context.validate()?;

        Ok(Self {
            workload_id: workload_id.to_string(),
            commit_id: commit_id.to_string(),
            observed_at: observed_at.to_string(),
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            target,
            budgets,
            confidence,
            validity,
            context,
        })
    }

    pub fn workload_id(&self) -> &str {
        &self.workload_id
    }

    pub fn commit_id(&self) -> &str {
        &self.commit_id
    }

    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    pub const fn p50_latency_us(&self) -> u64 {
        self.p50_latency_us
    }

    pub const fn p95_latency_us(&self) -> u64 {
        self.p95_latency_us
    }

    pub const fn error_count(&self) -> u32 {
        self.error_count
    }

    pub const fn sample_count(&self) -> u32 {
        self.sample_count
    }

    pub const fn target(&self) -> BenchmarkScenarioTarget {
        self.target
    }

    pub const fn budgets(&self) -> BenchmarkEvidenceBudgets {
        self.budgets
    }

    pub const fn confidence(&self) -> BenchmarkEvidenceConfidence {
        self.confidence
    }

    pub const fn validity(&self) -> BenchmarkEvidenceValidity {
        self.validity
    }

    pub const fn context(&self) -> BenchmarkEvidenceContext {
        self.context
    }

    /// Benchmark evidence is never authoritative for optimizer decisions.
    pub const fn is_authoritative(&self) -> bool {
        BENCHMARK_EVIDENCE_AUTHORITATIVE
    }

    /// Benchmark evidence is advisory input only and can never select a plan by itself.
    pub const fn can_select_plan_alone(&self) -> bool {
        BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE
    }

    pub const fn optimizer_consumption_role(&self) -> &'static str {
        BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY
    }

    pub fn validate_for_use_at(
        &self,
        now: EngineTimestamp,
    ) -> Result<(), BenchmarkScenarioEvidenceError> {
        if self.validity.is_not_yet_valid_at(now) {
            return Err(BenchmarkScenarioEvidenceError::NotYetValid);
        }
        if self.validity.is_expired_at(now) {
            return Err(BenchmarkScenarioEvidenceError::Expired);
        }
        Ok(())
    }

    pub fn validate_against_target(
        &self,
        expected_target: BenchmarkScenarioTarget,
    ) -> Result<(), BenchmarkScenarioEvidenceError> {
        expected_target.validate()?;
        if self.target.procedure_id != expected_target.procedure_id {
            return Err(BenchmarkScenarioEvidenceError::TargetProcedureIdMismatch);
        }
        if self.target.catalog_version != expected_target.catalog_version {
            return Err(BenchmarkScenarioEvidenceError::TargetCatalogVersionMismatch);
        }
        if self.target.contract_hash != expected_target.contract_hash {
            return Err(BenchmarkScenarioEvidenceError::TargetContractHashMismatch);
        }
        if self.target.stats_version != expected_target.stats_version {
            return Err(BenchmarkScenarioEvidenceError::TargetStatsVersionMismatch);
        }
        if self.target.plan_class != expected_target.plan_class {
            return Err(BenchmarkScenarioEvidenceError::TargetPlanClassMismatch);
        }
        Ok(())
    }

    pub fn validate_for_target_at(
        &self,
        expected_target: BenchmarkScenarioTarget,
        now: EngineTimestamp,
    ) -> Result<(), BenchmarkScenarioEvidenceError> {
        self.validate_for_use_at(now)?;
        self.validate_against_target(expected_target)
    }

    pub fn to_json(&self) -> String {
        json::scenario_evidence_to_json(self)
    }
}
