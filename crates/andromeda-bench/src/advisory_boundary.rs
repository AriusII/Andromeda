use std::collections::HashMap;

use crate::flat_json::{JsonField, escape_json_string};
use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
};

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

pub(crate) fn optional_u64_with_default(
    fields: &HashMap<String, JsonField>,
    name: &str,
    default: u64,
) -> Result<u64, String> {
    match fields.get(name) {
        Some(JsonField::Unsigned(value)) => Ok(*value),
        Some(JsonField::Null) | None => Ok(default),
        _ => Err(format!("invalid {name}")),
    }
}

pub(crate) fn optional_u32_with_default(
    fields: &HashMap<String, JsonField>,
    name: &str,
    default: u32,
) -> Result<u32, String> {
    match fields.get(name) {
        Some(JsonField::Unsigned(value)) => {
            u32::try_from(*value).map_err(|_| format!("{name} exceeds u32"))
        }
        Some(JsonField::Null) | None => Ok(default),
        _ => Err(format!("invalid {name}")),
    }
}
