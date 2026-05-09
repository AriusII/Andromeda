use std::collections::HashMap;

use andromeda_scenario_evidence::{flat_json::JsonField, validate_benchmark_advisory_only_fields};

pub(super) fn verify_advisory_fields(fields: &HashMap<String, JsonField>) -> Result<(), String> {
    validate_benchmark_advisory_only_fields(fields, "baseline")
}
