use crate::diagnostic_json::{json_option_u64, json_string};

use super::super::sensitive::redact_sensitive_cli_evidence;

pub(super) fn audit_output_text(value: &str) -> &str {
    redact_sensitive_cli_evidence(value)
}

pub(super) fn json_audit_string(value: &str) -> String {
    json_string(audit_output_text(value))
}

pub(super) fn json_option_audit_string(value: Option<&str>) -> String {
    value
        .map(json_audit_string)
        .unwrap_or_else(|| "null".to_string())
}

pub(super) fn json_optional_u64(value: Option<u64>) -> String {
    json_option_u64(value)
}

pub(super) fn json_raw_string(value: &str) -> String {
    json_string(value)
}

pub(super) fn option_u64_human(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}
