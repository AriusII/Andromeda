use crate::{
    BenchmarkEvidence, BenchmarkHardwareProfile, BenchmarkMeasurementMode, BenchmarkWorkloadClass,
    find_workload,
};

use super::errors::BenchmarkScenarioEvidenceError;

/// Observable provenance for benchmark-derived evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkEvidenceContext {
    workload_class: BenchmarkWorkloadClass,
    hardware_profile: Option<BenchmarkHardwareProfile>,
    measurement_mode: Option<BenchmarkMeasurementMode>,
    latency_source: Option<&'static str>,
    timing_source: Option<&'static str>,
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
            timing_source: None,
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
            timing_source: Some(evidence.timing_source),
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
        if self
            .timing_source
            .is_some_and(|timing_source| timing_source.trim().is_empty())
        {
            return Err(BenchmarkScenarioEvidenceError::EmptyTimingSource);
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

    pub const fn timing_source(self) -> Option<&'static str> {
        self.timing_source
    }

    pub const fn engine_harness(self) -> Option<&'static str> {
        self.engine_harness
    }

    pub const fn synthetic_model_version(self) -> Option<&'static str> {
        self.synthetic_model_version
    }
}
