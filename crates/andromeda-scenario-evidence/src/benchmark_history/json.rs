use std::collections::HashMap;

use crate::flat_json::{JsonField, json_optional_str, required_u64};

pub(super) use crate::flat_json::json_optional_u32;

pub(super) fn json_optional_string(value: Option<&str>) -> String {
    json_optional_str(value)
}

pub(super) fn required_u32_field(
    fields: &HashMap<String, JsonField>,
    name: &str,
) -> Result<u32, String> {
    u32::try_from(required_u64(fields, name)?).map_err(|_| format!("{name} exceeds u32"))
}
