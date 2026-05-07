//! Benchmark-side boundary for future `ScenarioEvidence` materialization.
//!
//! This module does not edit or construct catalog `ScenarioEvidence` directly.
//! It records the bounded evidence inputs that a future integration layer can
//! convert into `andromeda-catalog::scenario_evidence::ScenarioEvidence` after
//! resolving the catalog-side `StatsVersion` and `PlanClass` types.
//!
//! Boundary rules:
//!
//! - Evidence is advisory only. `is_authoritative()` always returns `false`.
//! - Duration, sample, and temp budgets are explicit and globally bounded.
//! - Confidence is a fixed `0..=1000` permille value, never a float.
//! - Validity is a half-open timestamp window: `[issued_at, expires_at)`.
//! - Validity is capped by `MAX_EVIDENCE_TTL_MS`.
//! - The target always names a non-zero `ProcedureId`, non-zero
//!   `CatalogVersion`, non-zero `ContractHash`, non-zero stats version, and
//!   one bounded plan class.
//! - Hardware profile, workload class, and measurement provenance are captured
//!   when the source evidence carries those fields.

use andromeda_core::{CatalogVersion, ContractHash, EngineTimestamp, ProcedureId};

use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkEvidence, BenchmarkHardwareProfile,
    BenchmarkHistoryRecord, BenchmarkMeasurementMode, BenchmarkRunRequest, BenchmarkWorkloadClass,
    MAX_DURATION_MS, MAX_EVIDENCE_TTL_MS, MAX_SAMPLES, MAX_TEMP_BYTES, find_workload,
    flat_json::escape_json_string,
};

const EVIDENCE_PERMILLE_MAX: u16 = 1_000;

/// Benchmark-local mirror of the catalog `StatsVersion` scalar.
///
/// The bench crate intentionally avoids depending on `andromeda-catalog`.
/// The integration layer must convert this value into the catalog type without
/// changing the raw number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BenchmarkStatsVersion(u64);

impl BenchmarkStatsVersion {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for BenchmarkStatsVersion {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Benchmark-local mirror of the catalog bounded `PlanClass` taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BenchmarkPlanClass {
    Singleton,
    ParameterShape,
    Cardinality,
    StatsAdaptive,
}

impl BenchmarkPlanClass {
    pub const VARIANT_COUNT: usize = 4;

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Singleton => 0x01,
            Self::ParameterShape => 0x02,
            Self::Cardinality => 0x03,
            Self::StatsAdaptive => 0x04,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Singleton => "singleton",
            Self::ParameterShape => "parameter-shape",
            Self::Cardinality => "cardinality",
            Self::StatsAdaptive => "stats-adaptive",
        }
    }
}

/// Fixed-scale confidence value for benchmark-derived evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BenchmarkEvidenceConfidence(u16);

impl BenchmarkEvidenceConfidence {
    pub const MAX_RAW: u16 = EVIDENCE_PERMILLE_MAX;

    pub const fn from_permille(value: u16) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if value > Self::MAX_RAW {
            Err(BenchmarkScenarioEvidenceError::ConfidenceOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

/// Half-open validity window for benchmark evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkEvidenceValidity {
    issued_at: EngineTimestamp,
    expires_at: EngineTimestamp,
}

impl BenchmarkEvidenceValidity {
    pub fn new(
        issued_at: EngineTimestamp,
        expires_at: EngineTimestamp,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if expires_at.is_zero() {
            return Err(BenchmarkScenarioEvidenceError::ExpiryMustBeNonZero);
        }
        if issued_at.as_unix_millis() >= expires_at.as_unix_millis() {
            return Err(BenchmarkScenarioEvidenceError::IssuedNotBeforeExpiry);
        }
        let ttl_ms = expires_at.as_unix_millis() - issued_at.as_unix_millis();
        if ttl_ms > MAX_EVIDENCE_TTL_MS {
            return Err(BenchmarkScenarioEvidenceError::ValidityWindowExceedsLimit);
        }
        Ok(Self {
            issued_at,
            expires_at,
        })
    }

    pub const fn issued_at(self) -> EngineTimestamp {
        self.issued_at
    }

    pub const fn expires_at(self) -> EngineTimestamp {
        self.expires_at
    }

    pub fn is_not_yet_valid_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() < self.issued_at.as_unix_millis()
    }

    pub fn is_expired_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() >= self.expires_at.as_unix_millis()
    }
}

/// Explicit resource budgets attached to evidence generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkEvidenceBudgets {
    pub duration_ms: u64,
    pub samples: u32,
    pub temp_bytes: u64,
}

impl BenchmarkEvidenceBudgets {
    pub fn new(
        duration_ms: u64,
        samples: u32,
        temp_bytes: u64,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if duration_ms == 0 {
            return Err(BenchmarkScenarioEvidenceError::ZeroDurationBudget);
        }
        if duration_ms > MAX_DURATION_MS {
            return Err(BenchmarkScenarioEvidenceError::DurationBudgetExceedsGlobalLimit);
        }
        if samples == 0 {
            return Err(BenchmarkScenarioEvidenceError::ZeroSampleBudget);
        }
        if samples > MAX_SAMPLES {
            return Err(BenchmarkScenarioEvidenceError::SampleBudgetExceedsGlobalLimit);
        }
        if temp_bytes == 0 {
            return Err(BenchmarkScenarioEvidenceError::ZeroTempBudget);
        }
        if temp_bytes > MAX_TEMP_BYTES {
            return Err(BenchmarkScenarioEvidenceError::TempBudgetExceedsGlobalLimit);
        }
        Ok(Self {
            duration_ms,
            samples,
            temp_bytes,
        })
    }

    pub fn from_run_request(
        request: &BenchmarkRunRequest,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        Self::new(
            request.duration_ms,
            request.samples,
            request.temp_budget_bytes,
        )
    }
}

/// Observable provenance for benchmark-derived evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkEvidenceContext {
    workload_class: BenchmarkWorkloadClass,
    hardware_profile: Option<BenchmarkHardwareProfile>,
    measurement_mode: Option<BenchmarkMeasurementMode>,
    latency_source: Option<&'static str>,
    engine_harness: Option<&'static str>,
    synthetic_model_version: Option<&'static str>,
}

impl BenchmarkEvidenceContext {
    pub fn from_history_record(workload_id: &str) -> Result<Self, BenchmarkScenarioEvidenceError> {
        let workload =
            find_workload(workload_id).ok_or(BenchmarkScenarioEvidenceError::UnknownWorkloadId)?;
        Ok(Self {
            workload_class: workload.workload_class,
            hardware_profile: None,
            measurement_mode: None,
            latency_source: None,
            engine_harness: None,
            synthetic_model_version: None,
        })
    }

    pub fn from_benchmark_evidence(
        evidence: &BenchmarkEvidence,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        let workload = find_workload(&evidence.workload_id)
            .ok_or(BenchmarkScenarioEvidenceError::UnknownWorkloadId)?;
        let context = Self {
            workload_class: workload.workload_class,
            hardware_profile: Some(evidence.hardware_profile),
            measurement_mode: Some(evidence.measurement_mode),
            latency_source: Some(evidence.latency_source),
            engine_harness: evidence.engine_harness,
            synthetic_model_version: evidence.synthetic_model_version,
        };
        context.validate()?;
        Ok(context)
    }

    pub fn validate(self) -> Result<(), BenchmarkScenarioEvidenceError> {
        if self
            .latency_source
            .is_some_and(|latency_source| latency_source.trim().is_empty())
        {
            return Err(BenchmarkScenarioEvidenceError::EmptyLatencySource);
        }
        if let Some(measurement_mode) = self.measurement_mode {
            let expected_mode = match self.workload_class {
                BenchmarkWorkloadClass::SyntheticDiagnostic => {
                    BenchmarkMeasurementMode::SyntheticDiagnostic
                }
                BenchmarkWorkloadClass::HarnessDiagnostic => {
                    BenchmarkMeasurementMode::HarnessDiagnostic
                }
                BenchmarkWorkloadClass::RealRuntime => {
                    return Err(BenchmarkScenarioEvidenceError::WorkloadMeasurementModeMismatch);
                }
            };
            if measurement_mode != expected_mode {
                return Err(BenchmarkScenarioEvidenceError::WorkloadMeasurementModeMismatch);
            }
        }
        if self.engine_harness.is_some() && self.synthetic_model_version.is_some() {
            return Err(BenchmarkScenarioEvidenceError::ConflictingSyntheticAndHarnessContext);
        }
        match self.measurement_mode {
            Some(BenchmarkMeasurementMode::SyntheticDiagnostic) => {
                if self.synthetic_model_version.is_none() {
                    return Err(BenchmarkScenarioEvidenceError::MissingSyntheticModelVersion);
                }
                if self.latency_source.is_none() {
                    return Err(BenchmarkScenarioEvidenceError::EmptyLatencySource);
                }
            }
            Some(BenchmarkMeasurementMode::HarnessDiagnostic) => {
                if self.engine_harness.is_none() {
                    return Err(BenchmarkScenarioEvidenceError::MissingEngineHarness);
                }
                if self.latency_source.is_none() {
                    return Err(BenchmarkScenarioEvidenceError::EmptyLatencySource);
                }
            }
            None => {}
        }
        Ok(())
    }

    pub const fn workload_class(self) -> BenchmarkWorkloadClass {
        self.workload_class
    }

    pub const fn hardware_profile(self) -> Option<BenchmarkHardwareProfile> {
        self.hardware_profile
    }

    pub const fn measurement_mode(self) -> Option<BenchmarkMeasurementMode> {
        self.measurement_mode
    }

    pub const fn latency_source(self) -> Option<&'static str> {
        self.latency_source
    }

    pub const fn engine_harness(self) -> Option<&'static str> {
        self.engine_harness
    }

    pub const fn synthetic_model_version(self) -> Option<&'static str> {
        self.synthetic_model_version
    }
}

/// Explicit target for benchmark-derived optimizer evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkScenarioTarget {
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub contract_hash: ContractHash,
    pub stats_version: BenchmarkStatsVersion,
    pub plan_class: BenchmarkPlanClass,
}

impl BenchmarkScenarioTarget {
    pub fn new(
        procedure_id: ProcedureId,
        catalog_version: CatalogVersion,
        contract_hash: ContractHash,
        stats_version: BenchmarkStatsVersion,
        plan_class: BenchmarkPlanClass,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        let target = Self {
            procedure_id,
            catalog_version,
            contract_hash,
            stats_version,
            plan_class,
        };
        target.validate()?;
        Ok(target)
    }

    pub fn validate(self) -> Result<(), BenchmarkScenarioEvidenceError> {
        if self.procedure_id.get() == 0 {
            return Err(BenchmarkScenarioEvidenceError::TargetProcedureIdZero);
        }
        if self.catalog_version.get() == 0 {
            return Err(BenchmarkScenarioEvidenceError::TargetCatalogVersionZero);
        }
        if self.contract_hash.is_zero() {
            return Err(BenchmarkScenarioEvidenceError::TargetContractHashZero);
        }
        if self.stats_version.is_zero() {
            return Err(BenchmarkScenarioEvidenceError::TargetStatsVersionZero);
        }
        Ok(())
    }
}

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
        format!(
            r#"{{"workload_id":"{}","commit_id":"{}","observed_at":"{}","p50_latency_us":{},"p95_latency_us":{},"error_count":{},"sample_count":{},"target_procedure_id":{},"target_catalog_version":{},"target_contract_hash":"{}","target_stats_version":{},"target_plan_class":"{}","target_plan_class_tag":{},"duration_budget_ms":{},"sample_budget":{},"temp_budget_bytes":{},"confidence_permille":{},"issued_at_unix_ms":{},"expires_at_unix_ms":{},"workload_class":"{}","hardware_profile":{},"measurement_mode":{},"latency_source":{},"engine_harness":{},"synthetic_model_version":{},"authoritative":{},"can_select_plan_alone":{},"optimizer_boundary":"{}"}}"#,
            escape_json_string(&self.workload_id),
            escape_json_string(&self.commit_id),
            escape_json_string(&self.observed_at),
            self.p50_latency_us,
            self.p95_latency_us,
            self.error_count,
            self.sample_count,
            self.target.procedure_id.get(),
            self.target.catalog_version.get(),
            self.target.contract_hash,
            self.target.stats_version.get(),
            self.target.plan_class.as_str(),
            self.target.plan_class.as_tag(),
            self.budgets.duration_ms,
            self.budgets.samples,
            self.budgets.temp_bytes,
            self.confidence.permille(),
            self.validity.issued_at().as_unix_millis(),
            self.validity.expires_at().as_unix_millis(),
            self.context.workload_class().as_str(),
            json_optional_str(
                self.context
                    .hardware_profile()
                    .map(|profile| profile.as_str())
            ),
            json_optional_str(self.context.measurement_mode().map(|mode| mode.as_str())),
            json_optional_str(self.context.latency_source()),
            json_optional_str(self.context.engine_harness()),
            json_optional_str(self.context.synthetic_model_version()),
            self.is_authoritative(),
            self.can_select_plan_alone(),
            escape_json_string(self.optimizer_consumption_role())
        )
    }
}

fn validate_budget_against_workload(
    workload_id: &str,
    budgets: BenchmarkEvidenceBudgets,
) -> Result<(), BenchmarkScenarioEvidenceError> {
    let workload =
        find_workload(workload_id).ok_or(BenchmarkScenarioEvidenceError::UnknownWorkloadId)?;
    if budgets.duration_ms > workload.max_duration_ms {
        return Err(BenchmarkScenarioEvidenceError::DurationBudgetExceedsWorkloadLimit);
    }
    if budgets.samples > workload.max_samples {
        return Err(BenchmarkScenarioEvidenceError::SampleBudgetExceedsWorkloadLimit);
    }
    if budgets.temp_bytes > workload.max_temp_bytes {
        return Err(BenchmarkScenarioEvidenceError::TempBudgetExceedsWorkloadLimit);
    }
    Ok(())
}

fn validate_history_advisory_metadata(
    record: &BenchmarkHistoryRecord,
    budgets: BenchmarkEvidenceBudgets,
) -> Result<(), BenchmarkScenarioEvidenceError> {
    let advisory = &record.advisory;
    if advisory.advisory_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
        return Err(BenchmarkScenarioEvidenceError::HistoryAdvisoryBoundaryInvalid);
    }
    if advisory.authoritative {
        return Err(BenchmarkScenarioEvidenceError::HistoryAuthoritative);
    }
    if advisory.can_select_plan_alone {
        return Err(BenchmarkScenarioEvidenceError::HistoryCanSelectPlanAlone);
    }
    if advisory.optimizer_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
        return Err(BenchmarkScenarioEvidenceError::HistoryOptimizerBoundaryInvalid);
    }
    if advisory.duration_cap_ms == 0 || advisory.duration_cap_ms > MAX_DURATION_MS {
        return Err(BenchmarkScenarioEvidenceError::HistoryDurationCapInvalid);
    }
    if advisory.sample_cap == 0 || advisory.sample_cap > MAX_SAMPLES {
        return Err(BenchmarkScenarioEvidenceError::HistorySampleCapInvalid);
    }
    if advisory.temp_cap_bytes == 0 || advisory.temp_cap_bytes > MAX_TEMP_BYTES {
        return Err(BenchmarkScenarioEvidenceError::HistoryTempCapInvalid);
    }
    if budgets.duration_ms > advisory.duration_cap_ms {
        return Err(BenchmarkScenarioEvidenceError::DurationBudgetExceedsHistoryCap);
    }
    if budgets.samples > advisory.sample_cap {
        return Err(BenchmarkScenarioEvidenceError::SampleBudgetExceedsHistoryCap);
    }
    if budgets.temp_bytes > advisory.temp_cap_bytes {
        return Err(BenchmarkScenarioEvidenceError::TempBudgetExceedsHistoryCap);
    }
    Ok(())
}

fn json_optional_str(value: Option<&str>) -> String {
    value
        .map(|value| format!(r#""{}""#, escape_json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

/// Closed reject reasons for benchmark-side evidence boundary validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkScenarioEvidenceError {
    EmptyWorkloadId,
    UnknownWorkloadId,
    EmptyCommitId,
    EmptyObservedAt,
    ZeroDurationBudget,
    DurationBudgetExceedsGlobalLimit,
    DurationBudgetExceedsWorkloadLimit,
    DurationBudgetExceedsHistoryCap,
    ZeroSampleBudget,
    SampleBudgetExceedsGlobalLimit,
    SampleBudgetExceedsWorkloadLimit,
    SampleBudgetExceedsHistoryCap,
    ZeroTempBudget,
    TempBudgetExceedsGlobalLimit,
    TempBudgetExceedsWorkloadLimit,
    TempBudgetExceedsHistoryCap,
    RecordSampleCountZero,
    RecordSampleCountExceedsBudget,
    ErrorCountExceedsSampleCount,
    LatencyPercentileOrderInvalid,
    ConfidenceOutOfRange,
    ExpiryMustBeNonZero,
    IssuedNotBeforeExpiry,
    ValidityWindowExceedsLimit,
    TargetProcedureIdZero,
    TargetCatalogVersionZero,
    TargetContractHashZero,
    TargetStatsVersionZero,
    TargetProcedureIdMismatch,
    TargetCatalogVersionMismatch,
    TargetContractHashMismatch,
    TargetStatsVersionMismatch,
    TargetPlanClassMismatch,
    EmptyLatencySource,
    MissingEngineHarness,
    MissingSyntheticModelVersion,
    ConflictingSyntheticAndHarnessContext,
    WorkloadMeasurementModeMismatch,
    HistoryAdvisoryBoundaryInvalid,
    HistoryAuthoritative,
    HistoryCanSelectPlanAlone,
    HistoryOptimizerBoundaryInvalid,
    HistoryDurationCapInvalid,
    HistorySampleCapInvalid,
    HistoryTempCapInvalid,
    NotYetValid,
    Expired,
}

impl core::fmt::Display for BenchmarkScenarioEvidenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyWorkloadId => {
                f.write_str("benchmark evidence workload id must not be empty")
            }
            Self::UnknownWorkloadId => {
                f.write_str("benchmark evidence workload id is not registered")
            }
            Self::EmptyCommitId => f.write_str("benchmark evidence commit id must not be empty"),
            Self::EmptyObservedAt => {
                f.write_str("benchmark evidence observed timestamp must not be empty")
            }
            Self::ZeroDurationBudget => {
                f.write_str("benchmark evidence duration budget must be greater than zero")
            }
            Self::DurationBudgetExceedsGlobalLimit => {
                f.write_str("benchmark evidence duration budget exceeds the global limit")
            }
            Self::DurationBudgetExceedsWorkloadLimit => {
                f.write_str("benchmark evidence duration budget exceeds the workload limit")
            }
            Self::DurationBudgetExceedsHistoryCap => {
                f.write_str("benchmark evidence duration budget exceeds the history record cap")
            }
            Self::ZeroSampleBudget => {
                f.write_str("benchmark evidence sample budget must be greater than zero")
            }
            Self::SampleBudgetExceedsGlobalLimit => {
                f.write_str("benchmark evidence sample budget exceeds the global limit")
            }
            Self::SampleBudgetExceedsWorkloadLimit => {
                f.write_str("benchmark evidence sample budget exceeds the workload limit")
            }
            Self::SampleBudgetExceedsHistoryCap => {
                f.write_str("benchmark evidence sample budget exceeds the history record cap")
            }
            Self::ZeroTempBudget => {
                f.write_str("benchmark evidence temp budget must be greater than zero")
            }
            Self::TempBudgetExceedsGlobalLimit => {
                f.write_str("benchmark evidence temp budget exceeds the global limit")
            }
            Self::TempBudgetExceedsWorkloadLimit => {
                f.write_str("benchmark evidence temp budget exceeds the workload limit")
            }
            Self::TempBudgetExceedsHistoryCap => {
                f.write_str("benchmark evidence temp budget exceeds the history record cap")
            }
            Self::RecordSampleCountZero => {
                f.write_str("benchmark evidence record must carry at least one sample")
            }
            Self::RecordSampleCountExceedsBudget => {
                f.write_str("benchmark evidence record sample count exceeds the sample budget")
            }
            Self::ErrorCountExceedsSampleCount => {
                f.write_str("benchmark evidence error count exceeds sample count")
            }
            Self::LatencyPercentileOrderInvalid => {
                f.write_str("benchmark evidence p95 latency must be >= p50 latency")
            }
            Self::ConfidenceOutOfRange => {
                f.write_str("benchmark evidence confidence must be in 0..=1000")
            }
            Self::ExpiryMustBeNonZero => {
                f.write_str("benchmark evidence validity expiry must be non-zero")
            }
            Self::IssuedNotBeforeExpiry => {
                f.write_str("benchmark evidence validity requires issued_at < expires_at")
            }
            Self::ValidityWindowExceedsLimit => {
                f.write_str("benchmark evidence validity window exceeds the configured limit")
            }
            Self::TargetProcedureIdZero => {
                f.write_str("benchmark evidence target ProcedureId must be non-zero")
            }
            Self::TargetCatalogVersionZero => {
                f.write_str("benchmark evidence target CatalogVersion must be non-zero")
            }
            Self::TargetContractHashZero => {
                f.write_str("benchmark evidence target ContractHash must be non-zero")
            }
            Self::TargetStatsVersionZero => {
                f.write_str("benchmark evidence target StatsVersion must be non-zero")
            }
            Self::TargetProcedureIdMismatch => {
                f.write_str("benchmark evidence target ProcedureId is not current")
            }
            Self::TargetCatalogVersionMismatch => {
                f.write_str("benchmark evidence target CatalogVersion is not current")
            }
            Self::TargetContractHashMismatch => {
                f.write_str("benchmark evidence target ContractHash is not current")
            }
            Self::TargetStatsVersionMismatch => {
                f.write_str("benchmark evidence target StatsVersion is not current")
            }
            Self::TargetPlanClassMismatch => {
                f.write_str("benchmark evidence target PlanClass is not current")
            }
            Self::EmptyLatencySource => {
                f.write_str("benchmark evidence latency source must not be empty")
            }
            Self::MissingEngineHarness => {
                f.write_str("benchmark evidence harness mode requires an engine harness")
            }
            Self::MissingSyntheticModelVersion => {
                f.write_str("benchmark evidence synthetic mode requires a model version")
            }
            Self::ConflictingSyntheticAndHarnessContext => {
                f.write_str("benchmark evidence cannot be synthetic and harness-backed")
            }
            Self::WorkloadMeasurementModeMismatch => {
                f.write_str("benchmark evidence workload class does not match measurement mode")
            }
            Self::HistoryAdvisoryBoundaryInvalid => {
                f.write_str("benchmark history advisory boundary must be advisory-only")
            }
            Self::HistoryAuthoritative => {
                f.write_str("benchmark history records must not be authoritative")
            }
            Self::HistoryCanSelectPlanAlone => {
                f.write_str("benchmark history records must not select plans alone")
            }
            Self::HistoryOptimizerBoundaryInvalid => {
                f.write_str("benchmark history optimizer boundary must be advisory-only")
            }
            Self::HistoryDurationCapInvalid => {
                f.write_str("benchmark history duration cap is outside global limits")
            }
            Self::HistorySampleCapInvalid => {
                f.write_str("benchmark history sample cap is outside global limits")
            }
            Self::HistoryTempCapInvalid => {
                f.write_str("benchmark history temp cap is outside global limits")
            }
            Self::NotYetValid => f.write_str("benchmark evidence is not yet valid"),
            Self::Expired => f.write_str("benchmark evidence has expired"),
        }
    }
}

impl std::error::Error for BenchmarkScenarioEvidenceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(value: u64) -> EngineTimestamp {
        EngineTimestamp::from_unix_millis(value)
    }

    fn target() -> BenchmarkScenarioTarget {
        BenchmarkScenarioTarget::new(
            ProcedureId::new(7),
            CatalogVersion::new(11),
            ContractHash::test_vector(0xAB),
            BenchmarkStatsVersion::new(3),
            BenchmarkPlanClass::ParameterShape,
        )
        .unwrap()
    }

    fn validity() -> BenchmarkEvidenceValidity {
        BenchmarkEvidenceValidity::new(ts(1_000), ts(2_000)).unwrap()
    }

    #[test]
    fn boundary_from_history_is_bounded_and_advisory() {
        let record = BenchmarkHistoryRecord::new(
            "vertical-v0-smoke".to_string(),
            "abc123".to_string(),
            "2026-05-06T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        );
        let evidence = BenchmarkScenarioEvidence::from_history_record(
            &record,
            target(),
            BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(750).unwrap(),
            validity(),
        )
        .unwrap();

        assert!(!evidence.is_authoritative());
        assert!(!evidence.can_select_plan_alone());
        assert_eq!(evidence.optimizer_consumption_role(), "advisory-only");
        assert_eq!(evidence.workload_id(), "vertical-v0-smoke");
        assert_eq!(evidence.target().procedure_id, ProcedureId::new(7));
        assert_eq!(evidence.target().catalog_version, CatalogVersion::new(11));
        assert_eq!(
            evidence.target().stats_version,
            BenchmarkStatsVersion::new(3)
        );
        assert_eq!(
            evidence.target().plan_class,
            BenchmarkPlanClass::ParameterShape
        );
        assert_eq!(evidence.confidence().permille(), 750);
        assert_eq!(
            evidence.context().workload_class(),
            BenchmarkWorkloadClass::SyntheticDiagnostic
        );
        assert_eq!(evidence.context().hardware_profile(), None);
        assert!(evidence.validate_for_use_at(ts(1_500)).is_ok());
        assert_eq!(
            evidence.validate_for_use_at(ts(2_000)).unwrap_err(),
            BenchmarkScenarioEvidenceError::Expired
        );

        let json = evidence.to_json();
        assert!(json.contains(r#""authoritative":false"#));
        assert!(json.contains(r#""can_select_plan_alone":false"#));
        assert!(json.contains(r#""optimizer_boundary":"advisory-only""#));
        assert!(json.contains(r#""target_catalog_version":11"#));
        assert!(json.contains(r#""target_plan_class":"parameter-shape""#));
        assert!(json.contains(r#""workload_class":"synthetic-diagnostic""#));
        assert!(json.contains(r#""hardware_profile":null"#));
        assert!(json.contains(r#""confidence_permille":750"#));
        assert!(json.contains(r#""temp_budget_bytes":1048576"#));
    }

    #[test]
    fn boundary_rejects_stale_or_untargeted_inputs() {
        assert_eq!(
            BenchmarkScenarioTarget::new(
                ProcedureId::new(0),
                CatalogVersion::new(1),
                ContractHash::test_vector(0xAB),
                BenchmarkStatsVersion::new(1),
                BenchmarkPlanClass::Singleton,
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::TargetProcedureIdZero
        );
        assert_eq!(
            BenchmarkScenarioTarget::new(
                ProcedureId::new(1),
                CatalogVersion::new(0),
                ContractHash::test_vector(0xAB),
                BenchmarkStatsVersion::new(1),
                BenchmarkPlanClass::Singleton,
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::TargetCatalogVersionZero
        );
        assert_eq!(
            BenchmarkScenarioTarget::new(
                ProcedureId::new(1),
                CatalogVersion::new(1),
                ContractHash::zero(),
                BenchmarkStatsVersion::new(1),
                BenchmarkPlanClass::Singleton,
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::TargetContractHashZero
        );
        assert_eq!(
            BenchmarkEvidenceConfidence::from_permille(1_001).unwrap_err(),
            BenchmarkScenarioEvidenceError::ConfidenceOutOfRange
        );
        assert_eq!(
            BenchmarkEvidenceValidity::new(ts(2_000), ts(2_000)).unwrap_err(),
            BenchmarkScenarioEvidenceError::IssuedNotBeforeExpiry
        );
    }

    #[test]
    fn boundary_rejects_records_that_exceed_sample_budget() {
        let record = BenchmarkHistoryRecord::new(
            "wal-append-file-smoke".to_string(),
            "abc123".to_string(),
            "2026-05-06T12:00:00Z".to_string(),
            100,
            500,
            0,
            21,
        );
        let error = BenchmarkScenarioEvidence::from_history_record(
            &record,
            target(),
            BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            BenchmarkScenarioEvidenceError::RecordSampleCountExceedsBudget
        );
    }

    #[test]
    fn boundary_rejects_unknown_workloads_and_workload_budget_overrides() {
        let unknown_record = BenchmarkHistoryRecord::new(
            "not-registered".to_string(),
            "abc123".to_string(),
            "2026-05-06T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        );
        assert_eq!(
            BenchmarkScenarioEvidence::from_history_record(
                &unknown_record,
                target(),
                BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
                BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
                validity(),
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::UnknownWorkloadId
        );

        let record = BenchmarkHistoryRecord::new(
            "protocol-smoke-contract".to_string(),
            "abc123".to_string(),
            "2026-05-06T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        );
        assert_eq!(
            BenchmarkScenarioEvidence::from_history_record(
                &record,
                target(),
                BenchmarkEvidenceBudgets::new(5_001, 20, 1024 * 1024).unwrap(),
                BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
                validity(),
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::DurationBudgetExceedsWorkloadLimit
        );
        assert_eq!(
            BenchmarkScenarioEvidence::from_history_record(
                &record,
                target(),
                BenchmarkEvidenceBudgets::new(5_000, 21, 1024 * 1024).unwrap(),
                BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
                validity(),
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::SampleBudgetExceedsWorkloadLimit
        );
        assert_eq!(
            BenchmarkScenarioEvidence::from_history_record(
                &record,
                target(),
                BenchmarkEvidenceBudgets::new(5_000, 20, crate::DEFAULT_TEMP_BYTES + 1).unwrap(),
                BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
                validity(),
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::TempBudgetExceedsWorkloadLimit
        );
    }

    #[test]
    fn boundary_rejects_unbounded_expiry() {
        assert_eq!(
            BenchmarkEvidenceValidity::new(ts(1_000), ts(1_000 + MAX_EVIDENCE_TTL_MS + 1))
                .unwrap_err(),
            BenchmarkScenarioEvidenceError::ValidityWindowExceedsLimit
        );
    }

    #[test]
    fn boundary_rejects_stale_target_versions() {
        let record = BenchmarkHistoryRecord::new(
            "vertical-v0-smoke".to_string(),
            "abc123".to_string(),
            "2026-05-06T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        );
        let evidence = BenchmarkScenarioEvidence::from_history_record(
            &record,
            target(),
            BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap();

        assert!(evidence.validate_for_target_at(target(), ts(1_500)).is_ok());

        let newer_stats_target = BenchmarkScenarioTarget::new(
            ProcedureId::new(7),
            CatalogVersion::new(11),
            ContractHash::test_vector(0xAB),
            BenchmarkStatsVersion::new(4),
            BenchmarkPlanClass::ParameterShape,
        )
        .unwrap();
        assert_eq!(
            evidence
                .validate_for_target_at(newer_stats_target, ts(1_500))
                .unwrap_err(),
            BenchmarkScenarioEvidenceError::TargetStatsVersionMismatch
        );
    }

    #[test]
    fn boundary_from_runtime_evidence_carries_profile_context() {
        let evidence = BenchmarkEvidence {
            workload_id: "protocol-smoke-contract".to_string(),
            workload_hypothesis: "typed protocol contract inspection should stay bounded without opening a network surface",
            workload_shape_version: "protocol-smoke-contract.synthetic.v1",
            workload_size: "contract-only synthetic inspection, samples<=20, duration_ms<=5000",
            primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
            baseline_ref: "history.protocol-smoke-contract.synthetic.v1",
            budget_origin: "static-workload-registry-v1",
            decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
            hardware_profile: BenchmarkHardwareProfile::DeclaredLocal,
            duration_ms: 1_000,
            samples: 5,
            warmups: 1,
            temp_budget_bytes: 1024 * 1024,
            started_at_unix_ms: 1,
            elapsed_ms: 6,
            sample_count: 5,
            p50_latency_us: 100,
            p95_latency_us: 200,
            error_count: 0,
            budget_status: crate::BudgetStatus::Passed,
            diagnostic_only: true,
            measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
            latency_source: "deterministic-latency-model",
            engine_harness: None,
            synthetic_model_version: Some("bounded-diagnostic-v1"),
        };

        let boundary = BenchmarkScenarioEvidence::from_benchmark_evidence(
            &evidence,
            "abc123",
            "2026-05-06T12:00:00Z",
            target(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap();

        assert_eq!(
            boundary.context().hardware_profile(),
            Some(BenchmarkHardwareProfile::DeclaredLocal)
        );
        assert_eq!(
            boundary.context().measurement_mode(),
            Some(BenchmarkMeasurementMode::SyntheticDiagnostic)
        );
        assert_eq!(
            boundary.context().synthetic_model_version(),
            Some("bounded-diagnostic-v1")
        );
        assert_eq!(boundary.context().engine_harness(), None);
        let json = boundary.to_json();
        assert!(json.contains(r#""hardware_profile":"declared-local""#));
        assert!(json.contains(r#""measurement_mode":"synthetic-diagnostic""#));
        assert!(json.contains(r#""synthetic_model_version":"bounded-diagnostic-v1""#));
    }

    #[test]
    fn boundary_rejects_incoherent_runtime_context() {
        let evidence = BenchmarkEvidence {
            workload_id: "btree-lookup-smoke".to_string(),
            workload_hypothesis: "mock B-Tree single-key lookup latency should remain stable over the bounded read-only key set",
            workload_shape_version: "btree-lookup-smoke.harness.v1",
            workload_size: "mock read-only key set, single-key lookup, samples<=20",
            primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
            baseline_ref: "history.btree-lookup-smoke.harness.v1",
            budget_origin: "static-workload-registry-v1",
            decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
            hardware_profile: BenchmarkHardwareProfile::Conservative,
            duration_ms: 1_000,
            samples: 5,
            warmups: 0,
            temp_budget_bytes: 1024 * 1024,
            started_at_unix_ms: 1,
            elapsed_ms: 5,
            sample_count: 5,
            p50_latency_us: 100,
            p95_latency_us: 200,
            error_count: 0,
            budget_status: crate::BudgetStatus::Passed,
            diagnostic_only: true,
            measurement_mode: BenchmarkMeasurementMode::HarnessDiagnostic,
            latency_source: "in-memory-btree-read-harness",
            engine_harness: None,
            synthetic_model_version: None,
        };

        assert_eq!(
            BenchmarkScenarioEvidence::from_benchmark_evidence(
                &evidence,
                "abc123",
                "2026-05-06T12:00:00Z",
                target(),
                BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
                validity(),
            )
            .unwrap_err(),
            BenchmarkScenarioEvidenceError::MissingEngineHarness
        );
    }

    #[test]
    fn plan_class_tags_match_the_catalog_boundary_contract() {
        assert_eq!(BenchmarkPlanClass::VARIANT_COUNT, 4);
        assert_eq!(BenchmarkPlanClass::Singleton.as_tag(), 0x01);
        assert_eq!(BenchmarkPlanClass::ParameterShape.as_tag(), 0x02);
        assert_eq!(BenchmarkPlanClass::Cardinality.as_tag(), 0x03);
        assert_eq!(BenchmarkPlanClass::StatsAdaptive.as_tag(), 0x04);
    }
}
