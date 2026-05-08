use std::collections::HashMap;

use crate::flat_json::JsonField;
use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, MAX_DURATION_MS, MAX_SAMPLES, MAX_TEMP_BYTES,
};

use crate::advisory_boundary::{
    optional_bool_with_default, optional_string_with_default, optional_u32_with_default,
    optional_u64_with_default,
};

/// Persisted advisory boundary metadata for history and regression records.
///
/// Benchmark history is optimizer input only. It must be explicit that these
/// records are not authoritative and cannot select a plan without the catalog
/// Procedure Store, statistics version, and active contract context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkHistoryAdvisoryMetadata {
    pub advisory_boundary: String,
    pub authoritative: bool,
    pub can_select_plan_alone: bool,
    pub optimizer_boundary: String,
    pub duration_cap_ms: u64,
    pub sample_cap: u32,
    pub temp_cap_bytes: u64,
}

impl BenchmarkHistoryAdvisoryMetadata {
    pub fn advisory_only_global_caps() -> Self {
        Self {
            advisory_boundary: BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY.to_string(),
            authoritative: BENCHMARK_EVIDENCE_AUTHORITATIVE,
            can_select_plan_alone: BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
            optimizer_boundary: BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY.to_string(),
            duration_cap_ms: MAX_DURATION_MS,
            sample_cap: MAX_SAMPLES,
            temp_cap_bytes: MAX_TEMP_BYTES,
        }
    }

    pub fn with_resource_caps(
        duration_cap_ms: u64,
        sample_cap: u32,
        temp_cap_bytes: u64,
    ) -> Result<Self, String> {
        let metadata = Self {
            duration_cap_ms,
            sample_cap,
            temp_cap_bytes,
            ..Self::advisory_only_global_caps()
        };
        metadata.validate()?;
        Ok(metadata)
    }

    pub(super) fn from_json_fields(fields: &HashMap<String, JsonField>) -> Result<Self, String> {
        let mut metadata = Self::advisory_only_global_caps();
        metadata.advisory_boundary = optional_string_with_default(
            fields,
            "advisory_boundary",
            BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
        )?;
        metadata.authoritative =
            optional_bool_with_default(fields, "authoritative", BENCHMARK_EVIDENCE_AUTHORITATIVE)?;
        metadata.can_select_plan_alone = optional_bool_with_default(
            fields,
            "can_select_plan_alone",
            BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
        )?;
        metadata.optimizer_boundary = optional_string_with_default(
            fields,
            "optimizer_boundary",
            BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
        )?;
        metadata.duration_cap_ms =
            optional_u64_with_default(fields, "duration_cap_ms", MAX_DURATION_MS)?;
        metadata.sample_cap = optional_u32_with_default(fields, "sample_cap", MAX_SAMPLES)?;
        metadata.temp_cap_bytes =
            optional_u64_with_default(fields, "temp_cap_bytes", MAX_TEMP_BYTES)?;
        metadata.validate()?;
        Ok(metadata)
    }

    fn validate(&self) -> Result<(), String> {
        if self.advisory_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
            return Err("benchmark history advisory_boundary must be advisory-only".to_string());
        }
        if self.authoritative {
            return Err("benchmark history records must not be authoritative".to_string());
        }
        if self.can_select_plan_alone {
            return Err("benchmark history records must not select plans alone".to_string());
        }
        if self.optimizer_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
            return Err("benchmark history optimizer_boundary must be advisory-only".to_string());
        }
        if self.duration_cap_ms == 0 || self.duration_cap_ms > MAX_DURATION_MS {
            return Err("benchmark history duration_cap_ms is outside global limits".to_string());
        }
        if self.sample_cap == 0 || self.sample_cap > MAX_SAMPLES {
            return Err("benchmark history sample_cap is outside global limits".to_string());
        }
        if self.temp_cap_bytes == 0 || self.temp_cap_bytes > MAX_TEMP_BYTES {
            return Err("benchmark history temp_cap_bytes is outside global limits".to_string());
        }
        Ok(())
    }
}
