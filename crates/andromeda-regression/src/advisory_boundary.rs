use std::collections::HashMap;

use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
};
use andromeda_scenario_evidence::flat_json::{JsonField, escape_json_string};

pub(crate) fn validate_advisory_only_fields(
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

pub(crate) fn json_optional_str(value: Option<&str>) -> String {
    value
        .map(|value| format!(r#""{}""#, escape_json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn optional_string_with_default(
    fields: &HashMap<String, JsonField>,
    name: &str,
    default: &str,
) -> Result<String, String> {
    match fields.get(name) {
        Some(JsonField::String(value)) => Ok(value.clone()),
        Some(JsonField::Null) | None => Ok(default.to_string()),
        _ => Err(format!("invalid {name}")),
    }
}

pub(crate) fn optional_bool_with_default(
    fields: &HashMap<String, JsonField>,
    name: &str,
    default: bool,
) -> Result<bool, String> {
    match fields.get(name) {
        Some(JsonField::Bool(value)) => Ok(*value),
        Some(JsonField::Null) | None => Ok(default),
        _ => Err(format!("invalid {name}")),
    }
}
