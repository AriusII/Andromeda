use std::collections::HashMap;

use crate::flat_json::{JsonField, optional_bool_with_default, optional_string_with_default};
use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
};

pub fn validate_benchmark_advisory_only_fields(
    fields: &HashMap<String, JsonField>,
    artifact: &str,
) -> Result<(), String> {
    let authoritative =
        optional_bool_with_default(fields, "authoritative", BENCHMARK_EVIDENCE_AUTHORITATIVE)?;
    if authoritative {
        return Err(format!("{artifact} records must not be authoritative"));
    }

    let can_select_plan_alone = optional_bool_with_default(
        fields,
        "can_select_plan_alone",
        BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    )?;
    if can_select_plan_alone {
        return Err(format!("{artifact} records must not select plans alone"));
    }

    let optimizer_boundary = optional_string_with_default(
        fields,
        "optimizer_boundary",
        BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
    )?;
    if optimizer_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
        return Err(format!(
            "{artifact} optimizer_boundary must be advisory-only"
        ));
    }

    Ok(())
}
