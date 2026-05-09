use std::collections::HashMap;

use crate::advisory_boundary::validate_advisory_only_fields;
use andromeda_scenario_evidence::flat_json::JsonField;

pub(super) fn verify_advisory_fields(fields: &HashMap<String, JsonField>) -> Result<(), String> {
    validate_advisory_only_fields(fields, "baseline")
}

pub(super) fn optional_u64(
    fields: &HashMap<String, JsonField>,
    name: &str,
) -> Result<Option<u64>, String> {
    match fields.get(name) {
        Some(JsonField::Unsigned(value)) => Ok(Some(*value)),
        Some(JsonField::Null) | None => Ok(None),
        _ => Err(format!("invalid {name}")),
    }
}

pub(super) fn optional_u32(
    fields: &HashMap<String, JsonField>,
    name: &str,
) -> Result<Option<u32>, String> {
    match fields.get(name) {
        Some(JsonField::Unsigned(value)) => u32::try_from(*value)
            .map(Some)
            .map_err(|_| format!("{name} exceeds u32")),
        Some(JsonField::Null) | None => Ok(None),
        _ => Err(format!("invalid {name}")),
    }
}
