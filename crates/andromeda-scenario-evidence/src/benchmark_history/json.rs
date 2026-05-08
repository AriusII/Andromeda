use std::collections::HashMap;

use crate::advisory_boundary::{
    json_optional_str, json_optional_u32 as advisory_json_optional_u32,
};
use crate::flat_json::{JsonField, required_u64};

pub(super) fn json_optional_string(value: Option<&str>) -> String {
    json_optional_str(value)
}

pub(super) fn json_optional_u32(value: Option<u32>) -> String {
    advisory_json_optional_u32(value)
}

pub(super) fn required_u32_field(
    fields: &HashMap<String, JsonField>,
    name: &str,
) -> Result<u32, String> {
    u32::try_from(required_u64(fields, name)?).map_err(|_| format!("{name} exceeds u32"))
}
